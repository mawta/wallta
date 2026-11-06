package app.wallta

import android.accounts.Account
import android.content.Context
import com.google.android.gms.auth.api.identity.AuthorizationRequest
import com.google.android.gms.auth.api.identity.Identity
import com.google.android.gms.common.GoogleApiAvailability
import com.google.android.gms.common.ConnectionResult
import com.google.android.gms.common.api.Scope
import com.google.android.gms.tasks.Tasks
import java.util.concurrent.TimeUnit

data class GoogleGrant(val account: String, val accessToken: String)

/** A system account chooser and Play Services consent; no browser OAuth or client secret. */
class GoogleAuthorization(private val context: Context) {
    companion object {
        fun available(context: Context) = GoogleApiAvailability.getInstance().isGooglePlayServicesAvailable(context) == ConnectionResult.SUCCESS
        fun scope(kind: String) = when (kind) {
            "drive" -> "https://www.googleapis.com/auth/drive.readonly"
            "photos" -> "https://www.googleapis.com/auth/photospicker.mediaitems.readonly"
            else -> error("Unsupported Google source")
        }
    }
    fun authorize(sourceId: String, kind: String, savedAccount: String? = null, interactive: Boolean = false): GoogleGrant {
        check(available(context)) { "Google Play services is unavailable. Use Connect with QR on this device." }
        val activity = context as? MainActivity
        val account = if (interactive) (activity ?: error("Open Wallta to connect Google")).chooseGoogleAccount(sourceId) else Account(savedAccount?.takeIf { it.contains('@') } ?: error("Reconnect Google in Wallta"), "com.google")
        val scopes = listOf(scope(kind), "https://www.googleapis.com/auth/userinfo.email")
        val client = Identity.getAuthorizationClient(context)
        val request = AuthorizationRequest.builder().setAccount(account).setRequestedScopes(scopes.map { Scope(it) }).build()
        var result = Tasks.await(client.authorize(request), 45, TimeUnit.SECONDS)
        if (result.hasResolution()) {
            check(interactive && activity != null) { "Google permission expired. Open Wallta and reconnect this account." }
            val intent = activity.resolveGoogleAuthorization(sourceId, result.pendingIntent ?: error("Google returned no authorization prompt"))
            result = client.getAuthorizationResultFromIntent(intent)
        }
        check(result.grantedScopes.containsAll(scopes)) { "Allow the requested Google permissions, then connect again." }
        return GoogleGrant(account.name, result.accessToken?.takeIf { it.isNotBlank() } ?: error("Google returned no access token. Connect again."))
    }
}
