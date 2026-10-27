<?php

namespace App\Http\Controllers;

use App\Services\DriveConnection;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\Crypt;
use Illuminate\Support\Facades\Http;

class DriveRelayController extends Controller
{
    private function configured(): void
    {
        abort_unless(config('services.wallta.client_id') && config('services.wallta.client_secret'), 503, 'Chủ relay cần cấu hình OAuth Google trước.');
    }

    private function pairKey(string $code): string { return 'wallta:pair:'.hash('sha256', $code); }
    private function verificationKey(string $code): string { return 'wallta:verify:'.hash('sha256', $code); }
    private function writePair(string $key, array $pair): void
    {
        Cache::put($key, Crypt::encryptString(json_encode($pair, JSON_THROW_ON_ERROR)), now()->addSeconds(max(1, $pair['expiresAt'] - time())));
    }
    private function readPair(string $key): array
    {
        $encrypted = Cache::get($key);
        abort_unless(is_string($encrypted), 410, 'Mã đã hết hạn. Tạo kết nối mới trên TV.');
        $pair = json_decode(Crypt::decryptString($encrypted), true, 32, JSON_THROW_ON_ERROR);
        abort_if($pair['expiresAt'] < time(), 410);
        return $pair;
    }
    private function keyFromVerification(string $code): string
    {
        abort_unless(preg_match('/^[a-f0-9]{64}$/D', $code), 410);
        $key = Cache::get($this->verificationKey($code));
        abort_unless(is_string($key), 410);
        return $key;
    }

    public function pair(Request $request)
    {
        $this->configured();
        $data = $request->validate(['folderId' => ['required', 'string', 'regex:/^[A-Za-z0-9_-]{1,200}$/D'], 'displayName' => ['required', 'string', 'max:100']]);
        $device = bin2hex(random_bytes(32)); $verification = bin2hex(random_bytes(32));
        $key = $this->pairKey($device);
        $this->writePair($key, ['phase' => 'authorizing', 'folderId' => $data['folderId'], 'displayName' => $data['displayName'], 'expiresAt' => time() + 900]);
        Cache::put($this->verificationKey($verification), $key, now()->addMinutes(15));
        return response()->json(['deviceCode' => $device, 'verificationUrl' => rtrim(config('app.url'), '/').'/connect/'.$verification, 'pollIntervalSeconds' => 5, 'expiresIn' => 900], 201)->header('Cache-Control', 'no-store');
    }

    public function poll(Request $request)
    {
        $data = $request->validate(['deviceCode' => ['required', 'string', 'size:64']]);
        $pair = $this->readPair($this->pairKey($data['deviceCode']));
        $result = ['phase' => $pair['phase'], 'pollIntervalSeconds' => 5, 'location' => $pair['folderId']];
        if ($pair['phase'] === 'ready') $result['token'] = $pair['token'];
        if (isset($pair['message'])) $result['message'] = $pair['message'];
        return response()->json($result)->header('Cache-Control', 'no-store');
    }

    public function cancelPair(Request $request)
    {
        $data = $request->validate(['deviceCode' => ['required', 'string', 'size:64']]);
        $key = $this->pairKey($data['deviceCode']);
        Cache::lock($key.':finalize', 10)->block(5, function () use ($key) {
            if (!Cache::has($key)) return;
            $pair = $this->readPair($key);
            if (isset($pair['token'])) {
                $connectionKey = DriveConnection::key($pair['token']);
                Cache::lock($connectionKey.':refresh', 40)->block(5, fn () => Cache::forget($connectionKey));
            }
            Cache::forget($key);
        });
        return response()->noContent()->header('Cache-Control', 'no-store');
    }

    public function confirm(string $code)
    {
        $pair = $this->readPair($this->keyFromVerification($code));
        abort_unless($pair['phase'] === 'authorizing', 410, 'Kết nối này đã hoàn tất.');
        return response()->view('connect', compact('pair', 'code'))->header('Cache-Control', 'no-store')->header('Referrer-Policy', 'no-referrer');
    }

    public function authorize(Request $request, string $code)
    {
        $this->configured();
        $key = $this->keyFromVerification($code); $pair = $this->readPair($key);
        abort_unless($pair['phase'] === 'authorizing', 410);
        $state = bin2hex(random_bytes(32)); $verifier = bin2hex(random_bytes(32));
        $request->session()->put('wallta.oauth', compact('key', 'state', 'verifier'));
        $challenge = rtrim(strtr(base64_encode(hash('sha256', $verifier, true)), '+/', '-_'), '=');
        $query = http_build_query(['client_id' => config('services.wallta.client_id'), 'redirect_uri' => rtrim(config('app.url'), '/').'/oauth/callback', 'response_type' => 'code', 'scope' => 'https://www.googleapis.com/auth/drive.readonly', 'access_type' => 'offline', 'prompt' => 'consent', 'state' => $state, 'code_challenge' => $challenge, 'code_challenge_method' => 'S256']);
        return redirect()->away('https://accounts.google.com/o/oauth2/v2/auth?'.$query)->header('Cache-Control', 'no-store')->header('Referrer-Policy', 'no-referrer');
    }

    public function callback(Request $request)
    {
        $oauth = $request->session()->pull('wallta.oauth');
        abort_unless(is_array($oauth) && is_string($request->query('state')) && hash_equals($oauth['state'], $request->query('state')), 403, 'Phiên đăng nhập không khớp.');
        $pair = $this->readPair($oauth['key']);
        abort_unless($pair['phase'] === 'authorizing', 410);
        if ($request->query('error')) {
            Cache::lock($oauth['key'].':finalize', 10)->block(5, function () use ($oauth) {
                $pair = $this->readPair($oauth['key']);
                abort_unless($pair['phase'] === 'authorizing', 410);
                $pair['phase'] = 'error'; $pair['message'] = 'Sếpp đã hủy cấp quyền Google.'; $this->writePair($oauth['key'], $pair);
            });
            return response('Đã hủy. Có thể đóng cửa sổ này.')->header('Cache-Control', 'no-store');
        }
        $data = $request->validate(['code' => ['required', 'string', 'max:4096']]);
        $response = Http::asForm()->timeout(30)->withOptions(['allow_redirects' => false])->post('https://oauth2.googleapis.com/token', ['client_id' => config('services.wallta.client_id'), 'client_secret' => config('services.wallta.client_secret'), 'redirect_uri' => rtrim(config('app.url'), '/').'/oauth/callback', 'grant_type' => 'authorization_code', 'code' => $data['code'], 'code_verifier' => $oauth['verifier']]);
        abort_unless($response->successful() && is_string($response->json('access_token')) && is_string($response->json('refresh_token')), 502, 'Google chưa cấp quyền lâu dài. Thử kết nối lại.');
        $folder = (new DriveConnection)->metadata($response->json('access_token'), $pair['folderId']);
        abort_unless(($folder['mimeType'] ?? '') === 'application/vnd.google-apps.folder' && is_string($folder['id'] ?? null), 403, 'Nguồn đã chọn phải là thư mục Google Drive.');
        Cache::lock($oauth['key'].':finalize', 10)->block(5, function () use ($oauth, $response, $folder) {
            // Network ran outside the lock. Cancellation or another browser may have won.
            $pair = $this->readPair($oauth['key']);
            abort_unless($pair['phase'] === 'authorizing', 410);
            $token = bin2hex(random_bytes(32));
            DriveConnection::put(DriveConnection::key($token), ['folderId' => $folder['id'], 'accessToken' => $response->json('access_token'), 'refreshToken' => $response->json('refresh_token'), 'expiresAt' => time() + (int) $response->json('expires_in', 3600)]);
            $pair['phase'] = 'ready'; $pair['token'] = $token; $pair['folderId'] = $folder['id']; $this->writePair($oauth['key'], $pair);
        });
        return response('Đã kết nối Google Drive với Wallta. Quay lại TV và lưu nguồn ảnh.')->header('Cache-Control', 'no-store')->header('Referrer-Policy', 'no-referrer');
    }

    public function files(Request $request, DriveConnection $drive)
    {
        [$key, $connection] = $drive->fromRequest($request);
        $data = $request->validate(['folderId' => ['required', 'string', 'regex:/^[A-Za-z0-9_-]{1,200}$/D'], 'pageToken' => ['nullable', 'string', 'max:4096']]);
        $token = $drive->token($key);
        $metadata = $drive->requireDescendant($token, $data['folderId'], $connection['folderId']);
        abort_unless(($metadata['mimeType'] ?? '') === 'application/vnd.google-apps.folder', 403);
        $response = Http::withToken($token)->timeout(30)->withOptions(['allow_redirects' => false])->get('https://www.googleapis.com/drive/v3/files', ['q' => "'".$data['folderId']."' in parents and trashed=false and (mimeType contains 'image/' or mimeType='application/vnd.google-apps.folder')", 'fields' => 'nextPageToken,files(id,name,mimeType)', 'pageSize' => 100, 'pageToken' => $data['pageToken'] ?? null, 'supportsAllDrives' => 'true', 'includeItemsFromAllDrives' => 'true']);
        abort_unless($response->successful(), 502, 'Không đọc được danh sách Drive.');
        return response()->json($response->json())->header('Cache-Control', 'no-store');
    }

    public function media(Request $request, string $id, DriveConnection $drive)
    {
        [$key, $connection] = $drive->fromRequest($request); $token = $drive->token($key);
        $metadata = $drive->requireDescendant($token, $id, $connection['folderId']);
        abort_unless(str_starts_with($metadata['mimeType'] ?? '', 'image/'), 415);
        $response = Http::withToken($token)->timeout(30)->withOptions(['allow_redirects' => false, 'stream' => true])->get('https://www.googleapis.com/drive/v3/files/'.rawurlencode($id), ['alt' => 'media', 'supportsAllDrives' => 'true']);
        abort_unless($response->successful(), 502, 'Không tải được ảnh Drive.');
        $stream = $response->toPsrResponse()->getBody(); $bytes = '';
        try {
            while (!$stream->eof()) {
                $bytes .= $stream->read(65536);
                abort_if(strlen($bytes) > 32 * 1024 * 1024, 413, 'Ảnh vượt giới hạn 32 MB.');
            }
        } finally { $stream->close(); }
        return response($bytes)->header('Content-Type', $metadata['mimeType'])->header('Cache-Control', 'no-store')->header('X-Content-Type-Options', 'nosniff');
    }

    public function disconnect(Request $request, DriveConnection $drive)
    {
        [$key] = $drive->fromRequest($request);
        Cache::lock($key.':refresh', 40)->block(5, fn () => Cache::forget($key));
        return response()->noContent()->header('Cache-Control', 'no-store');
    }
}
