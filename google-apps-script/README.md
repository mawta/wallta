# Wallta Google authorization broker

A dependency-free Google Apps Script project for QR authorization on Android TV/TV boxes. The same API can serve a browser-based phone fallback. The planned primary phone experience uses Google's native account/authorization UI instead.

**This service is not integrated into Wallta 0.1.11.** Deploy it, configure its separate Web OAuth client, then provide the `/exec` URL for app integration and live OAuth validation. Local protocol tests use simulated Apps Script services and Google responses; they do not prove a deployed callback works.

[Hướng dẫn triển khai tiếng Việt](DEPLOY.vi.md)

## Files

- `Code.gs`: copy into the Apps Script editor. No external libraries are required.
- `appsscript.json`: V8 manifest. The script itself requests only permission to make external requests.

Each visitor authorizes their own Google account through a **Web application** OAuth client. Do not use a Desktop client, the deployment owner's Google token, `DriveApp`, or `ScriptApp.getOAuthToken()`. Client ID/secret and deployment URL belong in Script Properties, outside this repository.

The broker stores per-device refresh tokens in private Script Properties. Only script editors can inspect this storage; they must be trusted. A short-lived Google access token is returned only to the device holding its connection capability. The refresh token and Web client secret never appear in device API replies, browser HTML, or verification URLs. Devices read image data directly from Google.

## Protocol 1

`GET /exec` returns `service: wallta-google-auth`, `protocol: 1`, configuration status and the exact callback URI. It exposes no credential values.

Native clients send `POST /exec` with `Content-Type: application/json`. Content Service redirects responses to a one-time `script.googleusercontent.com` URL. Follow Google's HTTPS response redirect as a GET; do not forward a POST body or credentials to arbitrary redirect destinations. This is a native HTTP API, not a browser CORS endpoint.

| Action | JSON fields | Successful `data` |
|---|---|---|
| `pair` | `kind`: `drive` or `photos`, `displayName` | `deviceCode`, `displayCode`, `verificationUrl`, `expiresIn`, `pollIntervalSeconds` |
| `poll` | `deviceCode` | `phase`; once ready: `connectionToken`, `account`, `kind` |
| `cancel` | `deviceCode` | Removes a pending pairing and an undelivered grant while its pairing record exists |
| `token` | `connectionToken` | `accessToken`, `expiresAt` (Unix seconds), `account`, `kind` |
| `disconnect` | `connectionToken` | Removes this device's broker connection |

POST replies are `{ "ok": true, "data": ... }` or `{ "ok": false, "error": { "code": "...", "message": "..." } }`. Apps Script Content Service does not set application HTTP error statuses; always inspect `ok`. Retry `busy`/temporary provider failures with backoff. Reconnect for `unauthorized`, `expired`, or revoked Google grants.

Pairing expires after ten minutes. Keep `deviceCode` private; the QR/browser URL contains a different verification code. Show `displayCode` on the device and have the user compare it before continuing. Poll at most once every five seconds. After receiving `connectionToken`, keep it in Android Keystore-backed storage. Saving a source promotes that connection locally. Before readiness, cancelling an unsaved source calls `cancel(deviceCode)`. Once a capability has been received, unsaved cancellation **must call `disconnect(connectionToken)`**, even if the pairing has expired or its record has been cleaned. Also cancel the pairing while it is still live. Do not cancel/disconnect a saved connection when closing its editor. Connection refresh and disconnect are separate operations.

Drive authorization is account-first; it requires no folder ID. Use the Google Drive API with the returned access token to list/select multiple folders later. Photos authorization grants **Photos Picker**, allowing only explicitly selected items. Create and poll a Picker session from the device, open its returned Google picker URL on the phone or TV's companion phone, and import the selected photos locally. This does not grant full-library/album synchronization or Ambient access.

Disconnect removes the device's broker record; it does not call Google's project-wide revoke endpoint or erase original photos. Google access tokens already issued to the device remain valid until their normal expiry. To revoke the entire app's Google access, use Google Account permissions.

## Limits and deployment status

This is a small-deployment option, subject to shared Apps Script quotas. It caps pending pairings at 50, stored connections at 100, new pairings at 100/hour and each JSON request at 8 KB. Pending/unclaimed grants are cleaned during pairing creation. Connections expire after thirty days without a successful refresh. Native clients should reuse cached access tokens instead of requesting one for each photo.

Before public distribution, verify the deployed callback/refresh flow, phone/TV integration and Google's consent requirements for the actual server-side token handling. The existing Windows verification submission describes Windows-local credential storage; it does not validate this new backend.

## Tests

From the repository root: `node --test scripts/apps-script.test.mjs`.

Tests exercise visitor-specific authorization, browser/device isolation, PKCE parameters, scope denial, callback replay, expiration/cancellation, concurrent disconnect/refresh, transient errors and bounded storage. No real credentials are used.

References: [Apps Script web apps](https://developers.google.com/apps-script/guides/web), [signed state callbacks](https://developers.google.com/apps-script/reference/script/state-token-builder), [Google's Apps Script OAuth2 library callback format](https://github.com/googleworkspace/apps-script-oauth2), [Content Service redirects](https://developers.google.com/apps-script/guides/content#redirects).
