<?php
namespace App\Services;

use Illuminate\Http\Request;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\Crypt;
use Illuminate\Support\Facades\Http;

class DriveConnection
{
    public static function key(string $token): string { return 'wallta:connection:'.hash('sha256', $token); }
    public static function put(string $key, array $connection): void
    {
        Cache::put($key, Crypt::encryptString(json_encode($connection, JSON_THROW_ON_ERROR)), now()->addDays(30));
    }
    public static function read(string $key): array
    {
        $encrypted = Cache::get($key);
        abort_unless(is_string($encrypted), 401, 'Quyền truy cập đã hết hạn. Kết nối lại từ Wallta.');
        return json_decode(Crypt::decryptString($encrypted), true, 32, JSON_THROW_ON_ERROR);
    }
    public function fromRequest(Request $request): array
    {
        $token = $request->bearerToken();
        abort_unless(is_string($token) && preg_match('/^[a-f0-9]{64}$/D', $token), 401);
        return [self::key($token), self::read(self::key($token))];
    }
    public function token(string $key): string
    {
        return Cache::lock($key.':refresh', 40)->block(5, function () use ($key) {
            $connection = self::read($key);
            if ($connection['expiresAt'] > time() + 60) return $connection['accessToken'];
            abort_unless(!empty($connection['refreshToken']), 401, 'Cần kết nối lại Google Drive.');
            $response = Http::asForm()->timeout(30)->withOptions(['allow_redirects' => false])->post('https://oauth2.googleapis.com/token', [
                'client_id' => config('services.wallta.client_id'), 'client_secret' => config('services.wallta.client_secret'),
                'refresh_token' => $connection['refreshToken'], 'grant_type' => 'refresh_token',
            ]);
            abort_unless($response->successful() && is_string($response->json('access_token')), 401, 'Google yêu cầu kết nối lại.');
            $connection['accessToken'] = $response->json('access_token');
            $connection['expiresAt'] = time() + (int) $response->json('expires_in', 3600);
            self::put($key, $connection);
            return $connection['accessToken'];
        });
    }
    public function metadata(string $token, string $id): array
    {
        abort_unless(preg_match('/^[A-Za-z0-9_-]{1,200}$/D', $id), 422, 'ID không hợp lệ.');
        $response = Http::withToken($token)->timeout(20)->withOptions(['allow_redirects' => false])->get('https://www.googleapis.com/drive/v3/files/'.rawurlencode($id), ['fields' => 'id,name,mimeType,parents,trashed', 'supportsAllDrives' => 'true']);
        abort_unless($response->successful(), 403, 'Không đọc được ảnh hoặc thư mục Drive.');
        $metadata = $response->json();
        abort_unless(is_array($metadata) && !($metadata['trashed'] ?? false), 403);
        return $metadata;
    }
    public function requireDescendant(string $token, string $id, string $root): array
    {
        $metadata = $this->metadata($token, $id);
        if ($id === $root) return $metadata;
        $parents = $metadata['parents'] ?? []; $visited = [];
        for ($depth = 0; $depth < 16 && $parents; $depth++) {
            $parent = array_shift($parents);
            if ($parent === $root) return $metadata;
            if (isset($visited[$parent])) continue;
            $visited[$parent] = true;
            $ancestor = $this->metadata($token, $parent);
            $parents = array_merge($parents, $ancestor['parents'] ?? []);
        }
        abort(403, 'Ảnh nằm ngoài thư mục đã cấp quyền cho thiết bị này.');
    }
}
