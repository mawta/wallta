<?php

namespace Tests\Feature;

use Illuminate\Support\Facades\Http;
use Illuminate\Support\Facades\Cache;
use App\Services\DriveConnection;
use Tests\TestCase;

class DrivePairingTest extends TestCase
{
    protected function setUp(): void
    {
        parent::setUp();
        config(['services.wallta.client_id' => 'test.apps.googleusercontent.com', 'services.wallta.client_secret' => 'secret', 'app.url' => 'https://relay.example']);
        Http::preventStrayRequests();
    }

    public function test_pairing_is_pending_until_the_browser_authorizes(): void
    {
        $pair = $this->postJson('/api/pair', ['folderId' => 'folder_123', 'displayName' => 'TV phòng khách'])->assertCreated()->json();
        $this->assertStringStartsWith('https://relay.example/connect/', $pair['verificationUrl']);
        $this->postJson('/api/pair/poll', ['deviceCode' => $pair['deviceCode']])->assertOk()->assertJson(['phase' => 'authorizing']);
        $this->postJson('/api/pair/poll', ['deviceCode' => str_repeat('x', 64)])->assertStatus(410);
    }

    public function test_callback_rejects_wrong_state_and_never_releases_credentials(): void
    {
        $this->get('/oauth/callback?state=wrong&code=abc')->assertStatus(403);
        Http::assertNothingSent();
    }

    public function test_private_drive_requires_a_capability(): void
    {
        $this->getJson('/api/drive/files?folderId=folder_123')->assertUnauthorized();
        $this->getJson('/api/drive/media/secret_file')->assertUnauthorized();
        Http::assertNothingSent();
    }

    public function test_folder_id_rejects_query_injection(): void
    {
        $this->postJson('/api/pair', ['folderId' => "x' or trashed=false", 'displayName' => 'TV'])->assertUnprocessable();
    }

    public function test_browser_consent_uses_pkce_and_returns_only_an_encrypted_folder_capability(): void
    {
        Http::fake(['https://oauth2.googleapis.com/token' => Http::response(['access_token' => 'google-access', 'refresh_token' => 'google-refresh', 'expires_in' => 3600]), 'https://www.googleapis.com/drive/v3/files/root_folder*' => Http::response(['id' => 'root_folder', 'mimeType' => 'application/vnd.google-apps.folder'])]);
        $pair = $this->postJson('/api/pair', ['folderId' => 'root_folder', 'displayName' => 'My TV'])->assertCreated()->json();
        $path = parse_url($pair['verificationUrl'], PHP_URL_PATH);
        $this->get($path)->assertOk()->assertSee('My TV');
        $consent = $this->post($path)->assertRedirect();
        parse_str(parse_url($consent->headers->get('Location'), PHP_URL_QUERY), $query);
        $this->assertSame('S256', $query['code_challenge_method']);
        $this->get('/oauth/callback?'.http_build_query(['state' => $query['state'], 'code' => 'test-code']))->assertOk();
        $ready = $this->postJson('/api/pair/poll', ['deviceCode' => $pair['deviceCode']])->assertOk()->assertJson(['phase' => 'ready', 'location' => 'root_folder'])->json();
        $this->assertMatchesRegularExpression('/^[a-f0-9]{64}$/D', $ready['token']);
        $serialized = json_encode($ready);
        $this->assertStringNotContainsString('google-access', $serialized);
        $this->assertStringNotContainsString('google-refresh', $serialized);
        $ciphertext = Cache::get(DriveConnection::key($ready['token']));
        $this->assertStringNotContainsString('google-refresh', $ciphertext);
        Http::assertSent(fn ($request) => str_contains($request->url(), 'oauth2.googleapis.com/token') && $request['grant_type'] === 'authorization_code' && hash('sha256', $request['code_verifier'], true) === base64_decode(strtr($query['code_challenge'], '-_', '+/')));
        $this->get('/oauth/callback?'.http_build_query(['state' => $query['state'], 'code' => 'test-code']))->assertForbidden();
    }

    public function test_a_tv_cannot_list_or_download_outside_its_selected_folder_and_disconnect_invalidates_it(): void
    {
        $capability = str_repeat('a', 64); $key = DriveConnection::key($capability);
        DriveConnection::put($key, ['folderId' => 'root', 'accessToken' => 'access', 'refreshToken' => 'refresh', 'expiresAt' => time() + 3600]);
        Http::fake([
            'https://www.googleapis.com/drive/v3/files/outside*' => Http::response(['id' => 'outside', 'mimeType' => 'image/png', 'parents' => ['different_root']]),
            'https://www.googleapis.com/drive/v3/files/different_root*' => Http::response(['id' => 'different_root', 'mimeType' => 'application/vnd.google-apps.folder', 'parents' => []]),
        ]);
        $this->withToken($capability)->getJson('/api/drive/files?folderId=outside')->assertForbidden();
        $this->withToken($capability)->getJson('/api/drive/media/outside')->assertForbidden();
        Http::assertNotSent(fn ($request) => str_contains($request->url(), 'alt=media'));
        $this->withToken($capability)->deleteJson('/api/connection')->assertNoContent();
        $this->assertNull(Cache::get($key));
        $this->withToken($capability)->getJson('/api/drive/files?folderId=root')->assertUnauthorized();
    }

    public function test_expired_access_is_refreshed_and_nested_folder_images_can_be_read(): void
    {
        $capability = str_repeat('b', 64); $key = DriveConnection::key($capability);
        DriveConnection::put($key, ['folderId' => 'root', 'accessToken' => 'expired', 'refreshToken' => 'refresh', 'expiresAt' => 1]);
        Http::fake([
            'https://oauth2.googleapis.com/token' => Http::response(['access_token' => 'fresh', 'expires_in' => 3600]),
            'https://www.googleapis.com/drive/v3/files/nested*' => Http::response(['id' => 'nested', 'mimeType' => 'application/vnd.google-apps.folder', 'parents' => ['root']]),
            'https://www.googleapis.com/drive/v3/files/image*' => function ($request) {
                return Http::response(str_contains($request->url(), 'alt=media') ? 'image-bytes' : ['id' => 'image', 'mimeType' => 'image/png', 'parents' => ['nested']]);
            },
            'https://www.googleapis.com/drive/v3/files?*' => Http::response(['files' => [['id' => 'image', 'name' => 'Photo', 'mimeType' => 'image/png']]]),
        ]);
        $this->withToken($capability)->getJson('/api/drive/files?folderId=nested')->assertOk()->assertJsonPath('files.0.id', 'image');
        $this->withToken($capability)->get('/api/drive/media/image')->assertOk()->assertContent('image-bytes')->assertHeader('Cache-Control', 'no-store, private');
        $this->assertSame('fresh', DriveConnection::read($key)['accessToken']);
        Http::assertSent(fn ($request) => $request->url() === 'https://oauth2.googleapis.com/token' && $request['grant_type'] === 'refresh_token');
    }

    public function test_denied_consent_becomes_an_actionable_poll_error(): void
    {
        $pair = $this->postJson('/api/pair', ['folderId' => 'root', 'displayName' => 'TV'])->json();
        $consent = $this->post(parse_url($pair['verificationUrl'], PHP_URL_PATH));
        parse_str(parse_url($consent->headers->get('Location'), PHP_URL_QUERY), $query);
        $this->get('/oauth/callback?'.http_build_query(['state' => $query['state'], 'error' => 'access_denied']))->assertOk();
        $this->postJson('/api/pair/poll', ['deviceCode' => $pair['deviceCode']])->assertJson(['phase' => 'error']);
        Http::assertNothingSent();
    }

    public function test_polling_does_not_consume_the_pairing_or_disconnect_rate_limits(): void
    {
        $pair = $this->postJson('/api/pair', ['folderId' => 'root', 'displayName' => 'TV'])->assertCreated()->json();
        for ($i = 0; $i < 11; $i++) $this->postJson('/api/pair/poll', ['deviceCode' => $pair['deviceCode']])->assertOk();
        $this->postJson('/api/pair', ['folderId' => 'root2', 'displayName' => 'TV 2'])->assertCreated();
    }

    public function test_cancelling_pending_pairing_prevents_later_browser_authorization(): void
    {
        $pair = $this->postJson('/api/pair', ['folderId' => 'root', 'displayName' => 'TV'])->assertCreated()->json();
        $path = parse_url($pair['verificationUrl'], PHP_URL_PATH);
        $this->deleteJson('/api/pair', ['deviceCode' => $pair['deviceCode']])->assertNoContent();
        $this->postJson('/api/pair/poll', ['deviceCode' => $pair['deviceCode']])->assertStatus(410);
        $this->post($path)->assertStatus(410);
        Http::assertNothingSent();
    }
}
