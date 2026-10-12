-- Print a running window's accessibility tree the way VoiceOver reads it:
-- role, then name (AXTitle, else AXDescription), then value, one element a
-- line. Usage:
--   osascript rust/crates/sp-app/tools/ax-tree.applescript <process name>
-- The process must be running and on screen. The terminal that runs this
-- needs Accessibility permission (System Settings, Privacy & Security).
on run argv
	set procName to item 1 of argv
	set out to ""
	tell application "System Events"
		tell process procName
			set frontmost to true
			delay 0.5
			set els to entire contents of window 1
			repeat with e in els
				set r to ""
				set n to ""
				set v to ""
				try
					set r to role of e
				end try
				try
					set n to title of e
				end try
				if n is missing value or n is "" then
					try
						set n to description of e
					end try
				end if
				try
					set v to value of e as text
				end try
				set out to out & r & " | " & n & " | " & v & linefeed
			end repeat
		end tell
	end tell
	return out
end run
