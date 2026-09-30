#!/bin/sh
# Turns the container's environment into rrn1-server's command line. Extra arguments are passed through.
exec /usr/local/bin/rrn1-server --port "${RRN1_PORT:-27750}" --bind 0.0.0.0 \
     --name "${RRN1_NAME:-Rave Racer}" --max-per-ip "${RRN1_MAX_PER_IP:-4}" "$@"
