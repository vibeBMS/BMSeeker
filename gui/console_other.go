//go:build !windows && !js

package main

import "os/exec"

func hideConsole(*exec.Cmd) {}
