package experience

import (
	"context"
	"fmt"
	"os/exec"
	"strconv"
	"testing"
)

// helperBuildJobs resolves the configured Cargo budget, including CPU-relative values.
func helperBuildJobs(cpus int, configured string) (int, error) {
	if configured == "" || configured == "default" {
		return cpus, nil
	}
	jobs, err := strconv.Atoi(configured)
	if err != nil || jobs == 0 {
		return 0, fmt.Errorf("invalid CARGO_BUILD_JOBS %q: expected a nonzero integer or default", configured)
	}
	if jobs < 0 {
		jobs = max(cpus+jobs, 1)
	}
	return jobs, nil
}

// helperBuildCommand cancels the owned build tree; Run, Output or Wait reaps the command.
func helperBuildCommand(ctx context.Context, name string, args ...string) *exec.Cmd {
	cmd := exec.CommandContext(ctx, name, args...)
	cmd.WaitDelay = helperPipeWaitDelay
	configureHelperBuildCancellation(cmd)
	return cmd
}

func TestHelperBuildJobs(t *testing.T) {
	for _, test := range []struct {
		configured string
		want       int
		wantErr    bool
	}{
		{configured: "", want: 8},
		{configured: "default", want: 8},
		{configured: "1", want: 1},
		{configured: "2", want: 2},
		{configured: "4", want: 4},
		{configured: "16", want: 16},
		{configured: "-2", want: 6},
		{configured: "-16", want: 1},
		{configured: "0", wantErr: true},
		{configured: "invalid", wantErr: true},
	} {
		t.Run(test.configured, func(t *testing.T) {
			jobs, err := helperBuildJobs(8, test.configured)
			if jobs != test.want || (err != nil) != test.wantErr {
				t.Fatalf("helperBuildJobs(8, %q) = %d, %v; want %d, error %v", test.configured, jobs, err, test.want, test.wantErr)
			}
		})
	}
}
