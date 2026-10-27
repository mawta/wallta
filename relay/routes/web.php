<?php

use Illuminate\Support\Facades\Route;
use App\Http\Controllers\DriveRelayController as Relay;

Route::get('/', fn () => response('Wallta Drive Relay. Kết nối từ ứng dụng Wallta.')->header('Cache-Control', 'no-store'));
Route::get('/connect/{code}', [Relay::class, 'confirm'])->middleware('throttle:30,1,wallta-confirm:');
Route::post('/connect/{code}', [Relay::class, 'authorize'])->middleware('throttle:10,1,wallta-authorize:');
Route::get('/oauth/callback', [Relay::class, 'callback'])->middleware('throttle:30,1,wallta-callback:');
