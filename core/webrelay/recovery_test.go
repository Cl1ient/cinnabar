package webrelay

import (
	"errors"
	"net"
	"net/netip"
	"testing"
	"testing/synctest"
	"time"
)

// failedControlConn fails a selected response while keeping cancellation and read deadlines real.
type failedControlConn struct {
	net.Conn
	failAt int
	writes int
}

// Write models a control connection lost before the selected response reaches the browser.
func (conn *failedControlConn) Write(data []byte) (int, error) {
	conn.writes++
	if conn.writes == conn.failAt {
		return 0, net.ErrClosed
	}
	return conn.Conn.Write(data)
}

func TestResumeSurvivesLostReplacementDelivery(t *testing.T) {
	for _, test := range []struct {
		name   string
		failAt int
	}{
		{name: "keepalive", failAt: 3},
		{name: "resumed_welcome", failAt: 1},
	} {
		t.Run(test.name, func(t *testing.T) {
			synctest.Test(t, func(t *testing.T) {
				relay := testRelay(t)
				client := netip.MustParseAddr("192.0.2.1")
				session, detach, err := relay.openControl(client, "", time.Now())
				if err != nil {
					t.Fatal(err)
				}
				lastKnown := session.token
				if test.failAt == 1 {
					detach(time.Now())
					session, detach, err = relay.openControl(client, lastKnown, time.Now())
					if err != nil {
						t.Fatal(err)
					}
				}
				server, browser := net.Pipe()
				defer server.Close()
				defer browser.Close()
				conn := &failedControlConn{Conn: server, failAt: test.failAt}
				done := make(chan struct{})
				go func() {
					defer close(done)
					relay.runControl(relay.ctx, conn, client, session)
				}()
				for range test.failAt - 1 {
					if err := readJSON(browser, new(controlEvent)); err != nil {
						t.Fatal(err)
					}
				}
				<-done
				if conn.writes != test.failAt {
					t.Fatal("control ended before the replacement write failed")
				}
				detach(time.Now())
				// Further lost welcome responses must preserve the browser's same last-known token.
				for range 3 {
					resumed, detach, err := relay.openControl(client, lastKnown, time.Now())
					if err != nil || resumed != session {
						t.Fatal("lost replacement made the retained account inaccessible", err)
					}
					if len(relay.tokens) != 2 {
						t.Fatal("resuming retained more than the current and presented bearers")
					}
					detach(time.Now())
				}
				relay.expire(time.Now().Add(idleTTL + time.Second))
				if len(relay.tokens) != 0 {
					t.Fatal("expired account retained a bearer")
				}
			})
		})
	}
}

func TestResumeOverlapKeepsOriginalExpiryAndGeneration(t *testing.T) {
	relay := testRelay(t)
	now := time.Now()
	client := netip.MustParseAddr("192.0.2.1")
	session, detach, err := relay.openControl(client, "", now)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { detach(now) }()
	previous := session.token
	replacement := relay.welcome(session, true, now.Add(resumeTTL/2))
	if _, _, err := relay.openControl(client, previous, now.Add(resumeTTL/2)); !errors.Is(err, errResume) {
		t.Fatal("overlapping bearer admitted a second controller", err)
	}
	if _, _, err := relay.playSession(nil, hello{Resume: previous, Generation: session.generation}, now.Add(resumeTTL/2)); err != nil {
		t.Fatal("overlapping bearer could not open play", err)
	}
	if _, _, err := relay.playSession(nil, hello{Resume: previous, Generation: session.generation + 1}, now); !errors.Is(err, errResume) {
		t.Fatal("overlapping bearer bypassed the play generation", err)
	}
	if _, _, err := relay.playSession(nil, hello{Resume: previous, Generation: session.generation}, now.Add(resumeTTL)); !errors.Is(err, errResume) {
		t.Fatal("rotation extended the previous bearer's expiry", err)
	}
	if _, _, err := relay.playSession(nil, hello{Resume: replacement.Resume, Generation: session.generation}, now.Add(resumeTTL)); err != nil {
		t.Fatal("replacement expired with the previous bearer", err)
	}
	detach(now)
	if _, _, err := relay.openControl(client, previous, now.Add(resumeTTL)); !errors.Is(err, errResume) {
		t.Fatal("expired previous bearer resumed control", err)
	}
}

func TestKeepaliveOverlapDiscardsOlderBearers(t *testing.T) {
	relay := testRelay(t)
	now := time.Now()
	client := netip.MustParseAddr("192.0.2.1")
	session, detach, err := relay.openControl(client, "", now)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { detach(now) }()
	previous := session.token
	discarded := ""
	for range 3 {
		current := relay.welcome(session, true, now).Resume
		if _, _, err := relay.playSession(nil, hello{Resume: previous, Generation: session.generation}, now); err != nil {
			t.Fatal("most recent bearer was discarded", err)
		}
		if len(relay.tokens) != 2 {
			t.Fatal("keepalives accumulated old bearers")
		}
		if discarded != "" {
			if _, _, err := relay.playSession(nil, hello{Resume: discarded, Generation: session.generation}, now); !errors.Is(err, errResume) {
				t.Fatal("an older bearer remained usable", err)
			}
		}
		discarded = previous
		previous = current
	}
}
