#!/usr/bin/env bash
SOCK=${XDG_RUNTIME_DIR}/quepasa.sock
PID=${XDG_RUNTIME_DIR}/quepasa.pid
CURLOPTS="-s --unix-socket $SOCK"
URL="http://local"
LOC=""
DBPATH="$1"

pid=$(cat $PID)
kill -0 $pid 2>/dev/null
if [[ $? -eq 1 ]]; then
	DBFILE=$(basename $DBPATH)
	echo | dmenu -p "Enter password:" | systemd-run --collect --pipe --user -u quepasa@$DBFILE quepasa -s $DBPATH
fi

while :; do 
	REPLY=$(curl $CURLOPTS "$URL" | jq -r '.[]' | dmenu)
	if [[ -z "$REPLY" ]]; then exit 1; fi
	LOC="$LOC/$REPLY"
	if [[ "$REPLY" != */ ]]; then
		curl $CURLOPTS \
			-H 'Content-Type: application/json' \
			-d "{ \"path\": \"$LOC\", \"method\": \"GetPassword\"}" \
			"$URL" | jq -r | xclip
		exit 0;
	fi
	URL="$URL/$LOC"
done
