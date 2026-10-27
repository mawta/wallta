<?php
use App\Http\Controllers\DriveRelayController as Relay;
use Illuminate\Support\Facades\Route;
Route::post('/pair', [Relay::class, 'pair'])->middleware('throttle:10,1,wallta-pair:');
Route::delete('/pair', [Relay::class, 'cancelPair'])->middleware('throttle:30,1,wallta-cancel:');
Route::post('/pair/poll', [Relay::class, 'poll'])->middleware('throttle:120,1,wallta-poll:');
Route::get('/drive/files', [Relay::class, 'files'])->middleware('throttle:120,1,wallta-files:');
Route::get('/drive/media/{id}', [Relay::class, 'media'])->middleware('throttle:240,1,wallta-media:');
Route::delete('/connection', [Relay::class, 'disconnect'])->middleware('throttle:30,1,wallta-disconnect:');
