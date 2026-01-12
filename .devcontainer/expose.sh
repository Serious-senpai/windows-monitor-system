#!/bin/sh
socat TCP-LISTEN:5601,fork,reuseaddr TCP:rabbitmq:5601 &
socat TCP-LISTEN:5672,fork,reuseaddr TCP:rabbitmq:5672 &
socat TCP-LISTEN:15672,fork,reuseaddr TCP:rabbitmq:15672 &

wait
