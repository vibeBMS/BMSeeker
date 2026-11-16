package main

import (
	"os/exec"
	"syscall"
)

// hideConsole keeps the CLI's console window from flashing up (Windows).
func hideConsole(cmd *exec.Cmd) {
	cmd.SysProcAttr = &syscall.SysProcAttr{HideWindow: true, CreationFlags: 0x08000000}
}
