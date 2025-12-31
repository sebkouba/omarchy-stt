clipboard should be restored after paste. it's not in the history but in the clipboard. the previous clipboard content should get restored. 

clipboard content should get saved and restored - The transcript shouldn't end up in the


### Scratch pad
removed from claude:

**CRITICAL WORKFLOW RULE:**
After completing any implementation changes, ALWAYS do the following before marking the task as complete or asking the user to test:
1. Run `cargo build --release` to build all binaries
2. Restart all daemons: `systemctl --user restart transcribe-daemon recording-daemon hotkey-daemon`
3. Check for warnings and fix them before completing the task

The user expects a working binary ready to test with all services updated. All three daemons must be restarted after any code changes to pick up the new compiled code.