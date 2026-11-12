#!/bin/sh
# Run on the authorized host as root (or through the documented one-shot Docker
# chroot helper). Only this application's vhost, webroot and renewal hook change.
set -eu
[ "$(id -u)" = 0 ] || { echo 'phone_web_requires_root' >&2; exit 1; }
project=/home/dks/phone-platform
site=/etc/nginx/sites-enabled/phone-platform.conf
state=$project/secrets/web-deploy
umask 077
mkdir -p "$state"
if [ -L "$site" ] || { [ -e "$site" ] && ! head -1 "$site" | grep -qx '# managed-by: phone-platform'; }; then
    echo 'phone_web_refuses_unmanaged_site' >&2
    exit 1
fi
install_site() {
    template=$1
    previous=$state/previous.conf
    had_previous=false
    if [ -f "$site" ]; then cp -p "$site" "$previous"; had_previous=true; fi
    install -m 644 "$template" "$site"
    if /usr/sbin/nginx -t && /usr/sbin/nginx -s reload; then
        echo 'phone_web_config_installed'
    else
        if [ "$had_previous" = true ]; then cp -p "$previous" "$site"; else rm -f "$site"; fi
        /usr/sbin/nginx -t && /usr/sbin/nginx -s reload
        echo 'phone_web_config_rolled_back' >&2
        exit 1
    fi
}
case "${1:-}" in
    http)
        mkdir -p /var/www/phone-platform-acme/.well-known/acme-challenge
        chmod 755 /var/www/phone-platform-acme /var/www/phone-platform-acme/.well-known /var/www/phone-platform-acme/.well-known/acme-challenge
        install_site "$project/infra/nginx/phone.bondscan.ru-http.conf"
        ;;
    certificate)
        /usr/bin/certbot certonly --non-interactive --agree-tos --register-unsafely-without-email \
            --webroot -w /var/www/phone-platform-acme -d phone.bondscan.ru \
            --cert-name phone.bondscan.ru --keep-until-expiring
        mkdir -p /etc/letsencrypt/renewal-hooks/deploy
        hook=/etc/letsencrypt/renewal-hooks/deploy/phone-platform-nginx.sh
        if [ -L "$hook" ] || { [ -e "$hook" ] && ! head -2 "$hook" | grep -qx '# managed-by: phone-platform'; }; then
            echo 'phone_web_refuses_unmanaged_hook' >&2; exit 1
        fi
        cat > "$hook" <<'HOOK'
#!/bin/sh
# managed-by: phone-platform
set -eu
case " ${RENEWED_DOMAINS:-} " in
    *' phone.bondscan.ru '*) /usr/sbin/nginx -t && /usr/sbin/nginx -s reload ;;
esac
HOOK
        chmod 755 "$hook"
        ;;
    https)
        test -s /etc/letsencrypt/live/phone.bondscan.ru/fullchain.pem
        test -s /etc/letsencrypt/live/phone.bondscan.ru/privkey.pem
        install_site "$project/infra/nginx/phone.bondscan.ru.conf"
        ;;
    check) /usr/sbin/nginx -t ;;
    renew-test) /usr/bin/certbot renew --cert-name phone.bondscan.ru --dry-run --non-interactive ;;
    *) echo 'Usage: deploy-web.sh http|certificate|https|check|renew-test' >&2; exit 2 ;;
esac
