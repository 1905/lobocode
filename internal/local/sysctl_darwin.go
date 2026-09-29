package local

import (
	"encoding/binary"
	"syscall"
)

func wiredLimitMiB() (int, error) {
	v, err := syscall.SysctlUint32("iogpu.wired_limit_mb")
	return int(v), err
}

// memBytes reads hw.memsize (uint64). syscall.Sysctl drops one trailing NUL byte, so pad back to 8.
func memBytes() (uint64, error) {
	s, err := syscall.Sysctl("hw.memsize")
	if err != nil {
		return 0, err
	}
	var b [8]byte
	copy(b[:], s)
	return binary.LittleEndian.Uint64(b[:]), nil
}

func sysctlString(name string) (string, error) { return syscall.Sysctl(name) }
