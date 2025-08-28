FROM elasticsearch:9.1.0 AS elasticsearch

COPY config/elasticsearch-certutil.txt /usr/share/elasticsearch/elasticsearch-certutil.txt
COPY config/elasticsearch.yml /usr/share/elasticsearch/config/elasticsearch.yml
# RUN bin/elasticsearch-certutil http < elasticsearch-certutil.txt && \
#     unzip /usr/share/elasticsearch/elasticsearch-ssl-http.zip -d /usr/share/elasticsearch/elasticsearch-ssl-http && \
#     rm /usr/share/elasticsearch/elasticsearch-ssl-http.zip && \
#     cp /usr/share/elasticsearch/elasticsearch-ssl-http/elasticsearch/http.p12 /usr/share/elasticsearch/config/http.p12 && \
RUN bin/elasticsearch & pid=$! && \
    until printf "y\nelastic-password\nelastic-password\n" | bin/elasticsearch-reset-password -i -u elastic; do sleep 3; done && \
    printf "y\nkibana-password\nkibana-password\n" | bin/elasticsearch-reset-password -i -u kibana_system && \
    kill -TERM $pid && \
    wait $pid || [ $? -eq 143 ]

FROM kibana:9.1.0 AS kibana

RUN bin/kibana-keystore create && \
    printf "kibana-password\n" | bin/kibana-keystore add -x elasticsearch.password

COPY config/kibana.yml /usr/share/kibana/config/kibana.yml
# COPY --from=elasticsearch /usr/share/elasticsearch/elasticsearch-ssl-http/kibana/elasticsearch-ca.pem /usr/share/kibana/config/elasticsearch-ca.pem
