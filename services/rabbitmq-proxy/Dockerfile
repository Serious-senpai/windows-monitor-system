# Reference: https://docs.docker.com/reference/dockerfile/
FROM haproxy:3.0-alpine AS runner

COPY services/rabbitmq-proxy/haproxy.conf /usr/local/etc/haproxy/haproxy.cfg
