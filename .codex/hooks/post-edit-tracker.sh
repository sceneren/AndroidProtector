#!/bin/bash
# PostToolUse hook: tracks edited project source files for review reminders.

stdin=$(cat)
file_path=$(echo "$stdin" | grep -o '"file_path":"[^"]*"' | head -1 | sed 's/"file_path":"//;s/"//')
session_id=$(echo "$stdin" | grep -o '"session_id":"[^"]*"' | head -1 | sed 's/"session_id":"//;s/"//')

# session_id 有效性检查
if [[ -z "$session_id" ]]; then
    exit 0
fi

if [[ "$file_path" =~ \.(ts|tsx|css|rs|java|xml|kts|cpp|h|c|cmake|toml|json)$ ]]; then
    track_file="/tmp/android_thirdgen_protector_edits_${session_id}.txt"
    echo "$file_path" >> "$track_file"
fi

exit 0
