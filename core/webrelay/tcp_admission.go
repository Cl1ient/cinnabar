package webrelay

import (
	"context"
	"crypto/tls"
	"net"
	"net/http"
	"net/netip"
	"sync"
	"time"
)

// admittedTCPListener counts sockets before TLS or HTTP parsing starts.
type admittedTCPListener struct {
	net.Listener
	relay *Relay
}

// Accept closes refused peers and retains admission through the accepted socket's lifetime.
func (listener *admittedTCPListener) Accept() (net.Conn, error) {
	for {
		conn, err := listener.Listener.Accept()
		if err != nil {
			return nil, err
		}
		address, err := netip.ParseAddrPort(conn.RemoteAddr().String())
		if err != nil {
			_ = conn.Close()
			continue
		}
		release, err := listener.relay.admit(address.Addr().Unmap(), time.Now())
		if err != nil {
			_ = conn.Close()
			continue
		}
		accepted := &admittedTCPConn{Conn: conn, relay: listener.relay, release: release, refs: 1}
		stop := context.AfterFunc(listener.relay.ctx, func() { _ = accepted.Close() })
		accepted.mu.Lock()
		if accepted.closed {
			accepted.mu.Unlock()
			stop()
		} else {
			accepted.stop = stop
			accepted.mu.Unlock()
		}
		return accepted, nil
	}
}

// admittedTCPConn releases admission after the socket and any upgraded handler have both ended.
type admittedTCPConn struct {
	net.Conn
	relay     *Relay
	release   func()
	mu        sync.Mutex
	refs      int
	closed    bool
	stop      func() bool
	closeOnce sync.Once
	closeErr  error
}

// retain counts a handler without charging the same TCP connection a second admission slot.
func (conn *admittedTCPConn) retain() (func(), error) {
	conn.mu.Lock()
	defer conn.mu.Unlock()
	if conn.closed {
		return nil, net.ErrClosed
	}
	conn.refs++
	return sync.OnceFunc(conn.drop), nil
}

// drop releases one owner, returning the admission slot only after the last owner finishes.
func (conn *admittedTCPConn) drop() {
	conn.mu.Lock()
	conn.refs--
	last := conn.refs == 0
	conn.mu.Unlock()
	if last {
		conn.release()
	}
}

// Close stops the relay cancellation hook, closes the socket once, and releases its ownership.
func (conn *admittedTCPConn) Close() error {
	conn.closeOnce.Do(func() {
		conn.mu.Lock()
		conn.closed = true
		stop := conn.stop
		conn.mu.Unlock()
		if stop != nil {
			stop()
		}
		conn.closeErr = conn.Conn.Close()
		conn.drop()
	})
	return conn.closeErr
}

// tcpAdmissionKey carries accept-time admission into HTTP requests without exposing it to peers.
type tcpAdmissionKey struct{}

// tcpConnContext finds the counted socket beneath ServeTLS's TLS wrapper.
func (relay *Relay) tcpConnContext(ctx context.Context, conn net.Conn) context.Context {
	if secure, ok := conn.(*tls.Conn); ok {
		conn = secure.NetConn()
	}
	if accepted, ok := conn.(*admittedTCPConn); ok && accepted.relay == relay {
		return context.WithValue(ctx, tcpAdmissionKey{}, accepted)
	}
	return ctx
}

// admitSocket retains accept-time admission, or admits an upgrade hosted by an external server.
func (relay *Relay) admitSocket(request *http.Request) (func(), error) {
	if conn, ok := request.Context().Value(tcpAdmissionKey{}).(*admittedTCPConn); ok && conn.relay == relay {
		return conn.retain()
	}
	return relay.admit(clientAddr(request), time.Now())
}
