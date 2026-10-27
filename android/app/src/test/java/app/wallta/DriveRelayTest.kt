package app.wallta

import org.junit.Assert.*
import org.junit.Test

class DriveRelayTest {
    @Test fun extractsFolderIdFromBrowserLink() {
        assertEquals("abc_123-xyz", DriveRelay.folder("https://drive.google.com/drive/folders/abc_123-xyz?usp=sharing"))
        assertEquals("abc_123-xyz", DriveRelay.folder("abc_123-xyz"))
    }
    @Test fun rejectsDriveQueriesInsteadOfTreatingThemAsIds() {
        assertThrows(IllegalArgumentException::class.java) { DriveRelay.folder("x' or trashed=false") }
    }
    @Test fun relayMustBeHttpsWithoutCredentialsQueryOrFragment() {
        assertEquals("https://relay.example/wallta", DriveRelay.base("https://relay.example/wallta/"))
        listOf("http://relay.example", "https://user:password@relay.example", "https://relay.example?token=x", "https://relay.example/#x", "file:///etc/passwd").forEach { url ->
            assertThrows(IllegalArgumentException::class.java) { DriveRelay.base(url) }
        }
    }
}
