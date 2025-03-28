#!/bin/env bash

existing_pid=$(netstat -tulpn 2>/dev/null | grep LISTEN | grep :3000 | awk '{print $7}' | cut -d'/' -f1)
if [[ -n "$existing_pid" ]]; then
    kill $existing_pid
    wait $existing_pid
fi

npm run debug &
pid=$!

trap "kill $pid; exit" SIGINT SIGTERM

while ! netstat -an | grep ESTABLISHED | grep :3000 > /dev/null; do
    sleep 1
done

while netstat -an | grep ESTABLISHED | grep :3000 > /dev/null; do
    sleep 1
done

kill $pid
wait $pid
