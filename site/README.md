# Wallta public OAuth pages

The HTML files here are served by a dedicated Nginx virtual host on `sva` at `https://wallta.mawta.store`. They are static and contain no analytics or application secrets. `nginx-http.conf` is for initial HTTP and ACME validation; `nginx-https.conf` adds the TLS virtual host and redirects HTTP.

## URLs for Google Auth Platform → Branding

| Field | Value |
| --- | --- |
| Application home page | `https://wallta.mawta.store/` |
| Application privacy policy link | `https://wallta.mawta.store/privacy.html` |
| Application terms of service link | `https://wallta.mawta.store/terms.html` |
| Authorized domain | `mawta.store` |

The owner must verify the **top private domain** `mawta.store` in Google Search Console with an account that is Owner or Editor of the OAuth project. A Cloudflare DNS TXT verification is the simplest way. Only then update Branding and resubmit; website availability alone does not mean Google has approved the app. See [Google's verification requirements](https://support.google.com/cloud/answer/13464321) and [domain guidance](https://developers.google.com/identity/verification/authentication-verification).

The owner chose to keep using the same Desktop OAuth client for a separate personal encrypted backup workflow with `drive.file` write access. Wallta itself requests `drive.readonly` for its photo features. The public homepage and privacy policy disclose the backup use without publishing the account, backup contents, token or server address. Sharing a client between unrelated products may still be rejected in Google's app review because the brand must accurately represent the application; disclosure does not guarantee approval. Do not publish credential files here.

## Deploy and check

Copy only `index.html`, `privacy.html`, `terms.html`, `style.css`, and `wallta.svg` to `/srv/wallta-site/public` as root-owned files with mode `644`; keep directories traversable by Nginx. The virtual host is `/etc/nginx/sites-available/wallta.mawta.store`, linked from `sites-enabled`. Save the previous Nginx config, run `nginx -t`, and reload Nginx for each config change. Certificate name is `wallta.mawta.store`; Certbot uses `/var/www/wallta-certbot` as its webroot. Check all three HTTPS URLs and the HTTP redirect, then check existing virtual hosts.
