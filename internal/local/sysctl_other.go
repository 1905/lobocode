//go:build !darwin

package local

import "errors"

var errNoSysctl = errors.New("sysctl: darwin only")

func wiredLimitMiB() (int, error)         { return 0, errNoSysctl }
func memBytes() (uint64, error)           { return 0, errNoSysctl }
func sysctlString(string) (string, error) { return "", errNoSysctl }
