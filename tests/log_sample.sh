#!/usr/bin/env bash
#
# @file log_sample.sh
# @brief Write synthetic log lines to a file so a log pane has something to follow
# @author doraemon-hub-art <1660219734@qq.com>
# @date 2026-10-06
# @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
#
#   tests/log_sample.sh [file] [interval] [lines]
#
#     file      where the lines are appended      (default /tmp/melogger-sample.log)
#     interval  seconds between two batches       (default 1; 0.2 and the like work)
#     lines     lines written per batch           (default 3)
#
#   ROTATE_EVERY=<n>   every n batches, move the file aside and let a new one start
#                      (default 0: never). This is what a log rotation looks like from
#                      the outside, and what the pane is supposed to notice.
#   LONG_EVERY=<n>     every n batches, one line long enough to need the sideways
#                      scroll (default 0: never).
#
# The lines have the shape the app writes itself — `2026-10-06 12:00:00.123 WARN  ssh`
# plus a message, five columns for the level and eight for the target — and some of them
# carry `closed` / `failed` / `rotated` / `resumed`, so the pane's search box has
# something to find. Ctrl-C stops it; the file stays where it is.
#
# Nothing here is specific to this machine: copy it to the host you want to follow a log
# on, and run it there.

set -eu

if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
	# The header above is the manual; print it and stop at the first line of code.
	awk 'NR > 1 && /^#/ { print } NR > 1 && !/^#/ { exit }' "$0"
	exit 0
fi

FILE=${1:-${TMPDIR:-/tmp}/melogger-sample.log}
INTERVAL=${2:-1}
LINES=${3:-3}
ROTATE_EVERY=${ROTATE_EVERY:-0}
LONG_EVERY=${LONG_EVERY:-0}

# level|target|message, the shape of one line
POOL=(
	"INFO|pty|session spawned (pid 41288, cols 96 rows 28)"
	"INFO|ssh|connected host=198.51.100.7:22 (rtt 31ms)"
	"INFO|ssh|tail resumed at offset 2981442"
	"INFO|ssh|reconnected, resuming tail -f"
	"DEBUG|filter|11 lines in buffer, 6 matched"
	"DEBUG|store|settings written"
	"INFO|font|4 families from the scan"
	"WARN|tail|file rotated, reopening /var/log/app.log"
	"WARN|pty|resize ignored: pty already closed (pid 41288)"
	"WARN|ssh|channel closed: broken pipe, retry in 5s (2/5)"
	"ERROR|ssh|auth failed for user=root"
	"ERROR|parse|line 10432 too long (8192B), truncated"
	"ERROR|serial|read /dev/ttyUSB0 failed: device disconnected"
)

mkdir -p "$(dirname "$FILE")"
printf '写入 %s\n' "$FILE"
[ "$ROTATE_EVERY" -gt 0 ] && printf '每 %s 批轮转一次（挪成 %s.1）\n' "$ROTATE_EVERY" "$FILE"
trap 'printf "\n停了：%s\n" "$FILE"; exit 0' INT TERM

batch=0
while :; do
	batch=$((batch + 1))
	stamp=$(date '+%Y-%m-%d %H:%M:%S.%3N')

	at=0
	while [ "$at" -lt "$LINES" ]; do
		entry=${POOL[$((RANDOM % ${#POOL[@]}))]}
		IFS='|' read -r level target message <<<"$entry"
		printf '%s %-5s %-8s %s\n' "$stamp" "$level" "$target" "$message" >>"$FILE"
		at=$((at + 1))
	done

	if [ "$LONG_EVERY" -gt 0 ] && [ $((batch % LONG_EVERY)) -eq 0 ]; then
		filler=$(printf 'field%d=value ' 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24)
		printf '%s %-5s %-8s %s\n' "$stamp" "WARN" "parse" "one very long line: ${filler}${filler}" >>"$FILE"
	fi

	if [ "$ROTATE_EVERY" -gt 0 ] && [ $((batch % ROTATE_EVERY)) -eq 0 ]; then
		# What logrotate does with create: the old one is kept, an empty new one takes
		# its place, so the path never stops existing.
		mv -f "$FILE" "$FILE.1"
		: >"$FILE"
		printf '第 %s 批：轮转，%s 挪到 %s.1，新的空文件已建\n' "$batch" "$FILE" "$FILE"
	fi

	sleep "$INTERVAL"
done
