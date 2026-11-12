FROM python:3.13-slim-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends openssh-client openssh-server ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 phonebridge \
    && useradd --uid 10001 --gid 10001 --create-home --home-dir /home/phonebridge --shell /bin/sh --password NP phonebridge \
    && mkdir -p /opt/phone-tunnel /var/lib/phone-tunnel /run/sshd \
    && chown 10001:10001 /var/lib/phone-tunnel /home/phonebridge
COPY infra/analyzer-tunnel/server.py infra/analyzer-tunnel/client.py /opt/phone-tunnel/
COPY infra/analyzer-tunnel/sshd_config /etc/phone-sshd_config
USER 10001:10001
WORKDIR /home/phonebridge
ENTRYPOINT ["python3", "/opt/phone-tunnel/client.py"]
