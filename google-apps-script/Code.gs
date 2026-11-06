/**
 * Wallta Google authorization broker for TV QR sign-in.
 * Deploy with the supplied manifest. Configuration belongs in Script Properties.
 * Never use ScriptApp.getOAuthToken() or DriveApp here: those access the owner.
 */
var WALLTA_PREFIX_ = 'WT1:';
var WALLTA_SCOPES_ = {
  drive: 'https://www.googleapis.com/auth/drive.readonly',
  photos: 'https://www.googleapis.com/auth/photospicker.mediaitems.readonly'
};

function doGet(e) {
  try {
    var p = e && e.parameter || {};
    if (p.action === 'connect') return walltaConnectPage_(p.code);
    walltaRequire_(!p.action || p.action === 'health', 'invalid_request', 'Use POST JSON for device requests.');
    var configured = false;
    try { walltaConfig_(); configured = true; } catch (_) {}
    return walltaJson_({service: 'wallta-google-auth', protocol: 1, configured: configured,
      providers: ['drive', 'photos'], redirectUri: walltaRedirectUri_()});
  } catch (error) {
    return e && e.parameter && e.parameter.action === 'connect' ?
      walltaPage_('Connection unavailable', walltaSafeError_(error).message) :
      walltaJson_({ok: false, error: walltaSafeError_(error)});
  }
}

function doPost(e) {
  try {
    var input = e && e.postData;
    walltaRequire_(input && /^application\/json(?:;|$)/i.test(input.type || '') &&
      typeof input.contents === 'string' && input.contents.length <= 8192,
      'invalid_request', 'Send a small application/json request.');
    var body;
    try { body = JSON.parse(input.contents); } catch (_) { walltaFail_('invalid_request', 'Invalid JSON.'); }
    walltaRequire_(body && typeof body === 'object' && !Array.isArray(body), 'invalid_request', 'Invalid request.');
    var data;
    switch (body.action) {
      case 'pair': data = walltaCreatePair_(body); break;
      case 'poll': data = walltaPoll_(body.deviceCode); break;
      case 'cancel': data = walltaCancel_(body.deviceCode); break;
      case 'token': data = walltaAccessToken_(body.connectionToken); break;
      case 'disconnect': data = walltaDisconnect_(body.connectionToken); break;
      default: walltaFail_('invalid_request', 'Unknown action.');
    }
    return walltaJson_({ok: true, data: data});
  } catch (error) {
    return walltaJson_({ok: false, error: walltaSafeError_(error)});
  }
}

function walltaConfig_() {
  var props = PropertiesService.getScriptProperties();
  var id = props.getProperty('GOOGLE_CLIENT_ID') || '';
  var secret = props.getProperty('GOOGLE_CLIENT_SECRET') || '';
  var url = props.getProperty('WALLTA_WEB_APP_URL') || '';
  walltaRequire_(/^[A-Za-z0-9_.-]+\.apps\.googleusercontent\.com$/.test(id) && secret.length > 0 &&
    /^https:\/\/script\.google\.com\/macros\/s\/[A-Za-z0-9_-]+\/exec$/.test(url),
    'not_configured', 'The service owner must configure the Web OAuth client and deployed /exec URL.');
  return {id: id, secret: secret, url: url};
}

function walltaRedirectUri_() {
  return 'https://script.google.com/macros/d/' + encodeURIComponent(ScriptApp.getScriptId()) + '/usercallback';
}
function walltaNow_() { return Math.floor(Date.now() / 1000); }
function walltaRandom_() { return (Utilities.getUuid() + Utilities.getUuid()).replace(/-/g, '').toLowerCase(); }
function walltaHash_(value) {
  return Utilities.computeDigest(Utilities.DigestAlgorithm.SHA_256, value, Utilities.Charset.UTF_8)
    .map(function (b) { return ('0' + (b & 255).toString(16)).slice(-2); }).join('');
}
function walltaKey_(type, token) {
  walltaRequire_(typeof token === 'string' && /^[a-f0-9]{64}$/.test(token), 'unauthorized', 'Connect Google again.');
  return WALLTA_PREFIX_ + type + ':' + walltaHash_(token);
}
function walltaRead_(key) {
  var raw = PropertiesService.getScriptProperties().getProperty(key);
  if (!raw) return null;
  try { return JSON.parse(raw); } catch (_) { return null; }
}
function walltaWrite_(key, value) {
  var text = JSON.stringify(value);
  walltaRequire_(text.length <= 4000, 'busy', 'Credential storage is full. Try again later.');
  PropertiesService.getScriptProperties().setProperty(key, text);
}
function walltaDelete_(key) { PropertiesService.getScriptProperties().deleteProperty(key); }
function walltaLocked_(callback) {
  var lock = LockService.getScriptLock();
  lock.waitLock(5000);
  try { return callback(); } finally { lock.releaseLock(); }
}
function walltaCleanupLocked_() {
  var all = PropertiesService.getScriptProperties().getProperties();
  Object.keys(all).forEach(function (key) {
    if (key.indexOf(WALLTA_PREFIX_) !== 0) return;
    var item;
    try { item = JSON.parse(all[key]); } catch (_) { walltaDelete_(key); return; }
    if (item.expiresAt > walltaNow_()) return;
    if (key.indexOf(WALLTA_PREFIX_ + 'P:') === 0) {
      walltaDelete_(item.verificationKey);
      if (item.connectionKey && !item.claimed) walltaDelete_(item.connectionKey);
    }
    walltaDelete_(key);
  });
}
function walltaPending_(key) {
  var item = walltaRead_(key);
  walltaRequire_(item && item.expiresAt > walltaNow_(), 'expired', 'Pairing expired. Start again in Wallta.');
  return item;
}

function walltaCreatePair_(body) {
  var config = walltaConfig_();
  walltaRequire_(body.kind === 'drive' || body.kind === 'photos', 'invalid_request', 'Choose Drive or Photos.');
  var name = typeof body.displayName === 'string' ? body.displayName.trim().slice(0, 80) : 'Wallta';
  return walltaLocked_(function () {
    walltaCleanupLocked_();
    var all = PropertiesService.getScriptProperties().getProperties();
    var keys = Object.keys(all);
    walltaRequire_(keys.filter(function (key) { return key.indexOf(WALLTA_PREFIX_ + 'P:') === 0; }).length < 50 &&
      keys.filter(function (key) { return key.indexOf(WALLTA_PREFIX_ + 'C:') === 0; }).length < 100,
      'busy', 'The sign-in service is busy. Try again later.');
    // Anonymous clients cannot supply an unlimited stream of persistent records.
    var hour = Math.floor(walltaNow_() / 3600);
    var rate = walltaRead_(WALLTA_PREFIX_ + 'RATE') || {hour: hour, count: 0};
    if (rate.hour !== hour) rate = {hour: hour, count: 0};
    walltaRequire_(rate.count < 100, 'busy', 'The sign-in service is busy. Try again later.');
    rate.count += 1; rate.expiresAt = (hour + 1) * 3600;
    walltaWrite_(WALLTA_PREFIX_ + 'RATE', rate);
    var deviceCode = walltaRandom_(), browserCode = walltaRandom_();
    var key = walltaKey_('P', deviceCode), verificationKey = walltaKey_('V', browserCode);
    var expires = walltaNow_() + 600;
    var displayCode = deviceCode.slice(0, 4).toUpperCase() + '-' + deviceCode.slice(4, 8).toUpperCase();
    walltaWrite_(key, {kind: body.kind, name: name || 'Wallta', displayCode: displayCode,
      phase: 'authorizing', expiresAt: expires, verificationKey: verificationKey});
    walltaWrite_(verificationKey, {pairKey: key, expiresAt: expires});
    return {deviceCode: deviceCode, displayCode: displayCode, expiresIn: 600, pollIntervalSeconds: 5,
      verificationUrl: config.url + '?action=connect&code=' + browserCode};
  });
}

function walltaConnectPage_(code) {
  var config = walltaConfig_();
  var authorization = walltaLocked_(function () {
    var reference = walltaPending_(walltaKey_('V', code));
    var pair = walltaPending_(reference.pairKey);
    walltaRequire_(pair.phase === 'authorizing', 'expired', 'This pairing has already finished.');
    // Reloads preserve an outstanding consent request; state and PKCE remain one-use.
    if (!pair.nonce) { pair.nonce = walltaRandom_(); pair.verifier = walltaRandom_(); walltaWrite_(reference.pairKey, pair); }
    var state = ScriptApp.newStateToken().withMethod('walltaOAuthCallback')
      .withArgument('pair', code).withArgument('nonce', pair.nonce)
      .withTimeout(Math.min(600, pair.expiresAt - walltaNow_())).createToken();
    var digest = Utilities.computeDigest(Utilities.DigestAlgorithm.SHA_256, pair.verifier, Utilities.Charset.UTF_8);
    var query = {client_id: config.id, redirect_uri: walltaRedirectUri_(), response_type: 'code',
      scope: 'openid https://www.googleapis.com/auth/userinfo.email ' + WALLTA_SCOPES_[pair.kind],
      access_type: 'offline', prompt: 'consent select_account', state: state,
      code_challenge: Utilities.base64EncodeWebSafe(digest).replace(/=+$/, ''), code_challenge_method: 'S256'};
    return {pair: pair, url: 'https://accounts.google.com/o/oauth2/v2/auth?' + Object.keys(query)
      .map(function (key) { return key + '=' + encodeURIComponent(query[key]); }).join('&')};
  });
  return walltaPage_('Connect Google', authorization.pair.name + ' · ' + authorization.pair.displayCode +
    '\nContinue only if this code matches the Wallta device you are connecting.', authorization.url, 'Continue with Google');
}

/** Called through the Apps Script signed /usercallback state route. */
function walltaOAuthCallback(e) {
  var key, nonce;
  try {
    var p = e && e.parameter || {};
    var config = walltaConfig_();
    var claim = walltaLocked_(function () {
      var reference = walltaPending_(walltaKey_('V', p.pair));
      key = reference.pairKey;
      var pair = walltaPending_(key);
      walltaRequire_(pair.phase === 'authorizing' && pair.nonce && pair.nonce === p.nonce,
        'expired', 'This sign-in request is no longer active.');
      nonce = pair.nonce;
      walltaRequire_(!p.error && typeof p.code === 'string' && p.code.length > 0 && p.code.length <= 4096,
        'google_denied', 'Google access was not granted. Connect again in Wallta.');
      pair.phase = 'exchanging'; walltaWrite_(key, pair);
      return pair;
    });
    var token = walltaGoogleJson_('https://oauth2.googleapis.com/token', {
      method: 'post', contentType: 'application/x-www-form-urlencoded',
      payload: {client_id: config.id, client_secret: config.secret, redirect_uri: walltaRedirectUri_(),
        code: p.code, code_verifier: claim.verifier, grant_type: 'authorization_code'}
    });
    walltaValidateAccess_(token);
    walltaRequire_(typeof token.refresh_token === 'string' && token.refresh_token.length > 0 &&
      String(token.scope || '').split(/\s+/).indexOf(WALLTA_SCOPES_[claim.kind]) !== -1,
      'google_denied', 'The requested Google permission was not granted. Connect again and allow it.');
    var identity = walltaGoogleJson_('https://www.googleapis.com/oauth2/v2/userinfo', {
      method: 'get', headers: {Authorization: 'Bearer ' + token.access_token}
    });
    walltaRequire_(typeof identity.email === 'string' && identity.email.indexOf('@') > 0 && identity.email.length <= 320,
      'google_denied', 'Google did not return the connected account. Connect again.');
    walltaLocked_(function () {
      var pair = walltaPending_(key);
      walltaRequire_(pair.phase === 'exchanging' && pair.nonce === nonce,
        'expired', 'This sign-in request was cancelled.');
      // Other pending callbacks may have filled capacity since this pair was created.
      var stored = Object.keys(PropertiesService.getScriptProperties().getProperties());
      walltaRequire_(stored.filter(function (key) { return key.indexOf(WALLTA_PREFIX_ + 'C:') === 0; }).length < 100,
        'busy', 'The sign-in service is full. Disconnect an unused device and try again.');
      var capability = walltaRandom_();
      var connectionKey = walltaKey_('C', capability);
      walltaWrite_(connectionKey, {kind: pair.kind, account: identity.email, accessToken: token.access_token,
        refreshToken: token.refresh_token, tokenExpiresAt: walltaNow_() + Number(token.expires_in),
        expiresAt: walltaNow_() + 30 * 86400});
      pair.phase = 'ready'; pair.account = identity.email;
      pair.connectionToken = capability; pair.connectionKey = connectionKey;
      delete pair.verifier; delete pair.nonce;
      walltaWrite_(key, pair);
    });
    return walltaPage_('Connected', identity.email + '\nReturn to Wallta. You can close this page.');
  } catch (error) {
    // Only the request that claimed this nonce may update its failure status.
    if (key && nonce) walltaLocked_(function () {
      var pair = walltaRead_(key);
      if (pair && pair.nonce === nonce && pair.expiresAt > walltaNow_()) {
        pair.phase = 'error'; pair.message = walltaSafeError_(error).message;
        delete pair.verifier; delete pair.nonce; walltaWrite_(key, pair);
      }
    });
    return walltaPage_('Connection failed', walltaSafeError_(error).message);
  }
}

function walltaPoll_(deviceCode) {
  return walltaLocked_(function () {
    var key = walltaKey_('P', deviceCode), pair = walltaPending_(key);
    if (pair.phase === 'ready') {
      var connection = walltaRead_(pair.connectionKey);
      walltaRequire_(connection && connection.expiresAt > walltaNow_(), 'unauthorized', 'Connect Google again.');
      pair.claimed = true; walltaWrite_(key, pair);
      return {phase: 'ready', kind: pair.kind, account: pair.account, connectionToken: pair.connectionToken};
    }
    return {phase: pair.phase === 'error' ? 'error' : 'authorizing', message: pair.message || '', pollIntervalSeconds: 5};
  });
}
function walltaCancel_(deviceCode) {
  return walltaLocked_(function () {
    var key = walltaKey_('P', deviceCode), pair = walltaRead_(key);
    if (pair) {
      if (pair.connectionKey) walltaDelete_(pair.connectionKey);
      walltaDelete_(pair.verificationKey); walltaDelete_(key);
    }
    return {cancelled: true};
  });
}
function walltaDisconnect_(capability) {
  return walltaLocked_(function () { walltaDelete_(walltaKey_('C', capability)); return {disconnected: true}; });
}

function walltaAccessToken_(capability) {
  var key = walltaKey_('C', capability);
  var claim = walltaLocked_(function () {
    var connection = walltaRead_(key);
    walltaRequire_(connection && connection.expiresAt > walltaNow_(), 'unauthorized', 'Connect Google again.');
    if (connection.tokenExpiresAt > walltaNow_() + 60) return {ready: connection};
    walltaRequire_(!(connection.refreshUntil > walltaNow_()), 'busy', 'Token renewal is in progress. Retry shortly.');
    connection.refreshNonce = walltaRandom_(); connection.refreshUntil = walltaNow_() + 90;
    walltaWrite_(key, connection); return {refresh: connection};
  });
  if (claim.ready) return walltaPublicToken_(claim.ready);
  try {
    var config = walltaConfig_();
    var token = walltaGoogleJson_('https://oauth2.googleapis.com/token', {
      method: 'post', contentType: 'application/x-www-form-urlencoded',
      payload: {client_id: config.id, client_secret: config.secret,
        refresh_token: claim.refresh.refreshToken, grant_type: 'refresh_token'}
    });
    walltaValidateAccess_(token);
    return walltaLocked_(function () {
      var current = walltaRead_(key);
      walltaRequire_(current && current.refreshNonce === claim.refresh.refreshNonce && current.expiresAt > walltaNow_(),
        'unauthorized', 'This Google connection is no longer active.');
      current.accessToken = token.access_token; current.tokenExpiresAt = walltaNow_() + Number(token.expires_in);
      if (typeof token.refresh_token === 'string' && token.refresh_token) current.refreshToken = token.refresh_token;
      current.expiresAt = walltaNow_() + 30 * 86400;
      delete current.refreshNonce; delete current.refreshUntil;
      walltaWrite_(key, current); return walltaPublicToken_(current);
    });
  } catch (error) {
    walltaLocked_(function () {
      var current = walltaRead_(key);
      if (!current || current.refreshNonce !== claim.refresh.refreshNonce) return;
      if (error.walltaCode === 'google_denied') walltaDelete_(key);
      else { delete current.refreshNonce; delete current.refreshUntil; walltaWrite_(key, current); }
    });
    throw error;
  }
}
function walltaPublicToken_(connection) {
  return {accessToken: connection.accessToken, expiresAt: connection.tokenExpiresAt,
    kind: connection.kind, account: connection.account};
}
function walltaValidateAccess_(token) {
  walltaRequire_(typeof token.access_token === 'string' && token.access_token.length > 0 &&
    Number.isFinite(Number(token.expires_in)) && Number(token.expires_in) >= 60 && Number(token.expires_in) <= 86400,
    'provider_error', 'Google returned an invalid token response. Try connecting again.');
}
function walltaGoogleJson_(url, options) {
  options.muteHttpExceptions = true; options.followRedirects = false; options.validateHttpsCertificates = true;
  var response = UrlFetchApp.fetch(url, options), status = response.getResponseCode();
  if (status === 400 || status === 401 || status === 403) walltaFail_('google_denied', 'Google requires a new connection. Sign in again.');
  walltaRequire_(status >= 200 && status < 300, 'provider_error', 'Google is temporarily unavailable. Try again.');
  var text = response.getContentText();
  walltaRequire_(text.length <= 65536, 'provider_error', 'Google returned an invalid response.');
  var json;
  try { json = JSON.parse(text); } catch (_) { walltaFail_('provider_error', 'Google returned an invalid response.'); }
  walltaRequire_(json && typeof json === 'object' && !Array.isArray(json), 'provider_error', 'Google returned an invalid response.');
  return json;
}
function walltaFail_(code, message) { var error = new Error(message); error.walltaCode = code; throw error; }
function walltaRequire_(value, code, message) { if (!value) walltaFail_(code, message); }
function walltaSafeError_(error) {
  return error && error.walltaCode ? {code: error.walltaCode, message: error.message} :
    {code: 'service_error', message: 'The sign-in service could not complete this request. Try again later.'};
}
function walltaJson_(value) { return ContentService.createTextOutput(JSON.stringify(value)).setMimeType(ContentService.MimeType.JSON); }
function walltaEscape_(text) {
  return String(text).replace(/[&<>"']/g, function (character) {
    return {'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[character];
  });
}
function walltaPage_(title, message, link, label) {
  var html = '<!doctype html><html><head><meta name="referrer" content="no-referrer"><base target="_top">' +
    '<style>body{margin:0;background:#10170f;color:#e4ebdf;font:16px/1.6 system-ui,sans-serif;display:grid;min-height:90vh;place-items:center}' +
    'main{max-width:420px;margin:24px;padding:28px;border:1px solid #3a4934;border-radius:16px;background:#1b2518}' +
    'h1{font-size:24px;margin:0 0 12px}p{white-space:pre-line;overflow-wrap:anywhere;color:#bdcbb5}' +
    'a{display:block;padding:12px 18px;border-radius:8px;background:#fff;color:#252a30;text-align:center;text-decoration:none;font-weight:600}</style></head>' +
    '<body><main><h1>' + walltaEscape_(title) + '</h1><p>' + walltaEscape_(message) + '</p>' +
    (link ? '<a target="_top" rel="noreferrer" href="' + walltaEscape_(link) + '">' + walltaEscape_(label) + '</a>' : '') +
    '</main></body></html>';
  return HtmlService.createHtmlOutput(html).setTitle('Wallta · ' + title).addMetaTag('viewport', 'width=device-width, initial-scale=1');
}
