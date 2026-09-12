-- UTM 4.7.4 CLI can return before a slow guest command has exited.
-- Poll the documented `exited` property and preserve a nonzero exit status.
on run argv
    if (count argv) < 2 then error "usage: osascript exec.applescript UUID /program [args...]"
    set vmId to item 1 of argv
    set programPath to item 2 of argv
    set commandArgs to {}
    if (count argv) > 2 then set commandArgs to items 3 thru -1 of argv
    tell application "UTM"
        set vm to virtual machine id vmId
        set proc to execute vm at programPath with arguments commandArgs output capturing true
        set deadline to (current date) + 600
        repeat
            set statusRecord to get result proc
            if exited of statusRecord then exit repeat
            if (current date) > deadline then error "Timed out waiting for guest result; the guest process may still be running."
            delay 0.2
        end repeat
        set output to ""
        try
            set output to output text of statusRecord
        end try
        try
            set output to output & error text of statusRecord
        end try
        if exit code of statusRecord is not 0 then error output & " (guest command failed)"
        return output
    end tell
end run
