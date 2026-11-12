FROM python:3.13-slim-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends openssh-client openssh-server ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 phonebridge \
    && useradd --uid 10001 --gid 10001 --create-home --home-dir /home/phonebridge --shell /bin/sh --password NP phonebridge \
    && mkdir -p /opt/phone-tunnel /var/lib/phone-tunnel /run/sshd \
    && chown 10001:10001 /var/lib/phone-tunnel /home/phonebridge
COPY --chmod=0644 infra/analyzer-tunnel/server.py infra/analyzer-tunnel/client.py /opt/phone-tunnel/
COPY --chmod=0644 infra/analyzer-tunnel/sshd_config /etc/phone-sshd_config
USER 10001:10001
WORKDIR /home/phonebridge
# Assert runtime-user access during the build, including the SSH configuration.
RUN python3 -c "from pathlib import Path; files=list(Path('/opt/phone-tunnel').glob('*.py')); assert len(files)==2; [compile(p.read_text(), str(p), 'exec') for p in files]; Path('/etc/phone-sshd_config').read_text()"
ENTRYPOINT ["python3", "/opt/phone-tunnel/client.py"]
