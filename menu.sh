#!/usr/bin/env bash
SOCK=${XDG_RUNTIME_DIR}/quepasa.sock
CURLOPTS="-s --unix-socket $SOCK"
URL="http://local"
LOC=""

while :; do 
	REPLY=$(curl $CURLOPTS "$URL" | jq -r '.[]' | dmenu)
	if [[ -z "$REPLY" ]]; then exit 1; fi
	LOC="$LOC/$REPLY"
	if [[ "$REPLY" != */ ]]; then
		curl $CURLOPTS \
			-H 'Content-Type: application/json' \
			-d "{ \"path\": \"$LOC\", \"method\": \"GetPassword\"}" \
			"$URL" | jq -r
		exit 0;
	fi
	URL="$URL/$LOC"
done
