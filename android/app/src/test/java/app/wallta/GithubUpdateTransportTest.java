package app.wallta;
import org.junit.Test;
import static org.junit.Assert.*;
public class GithubUpdateTransportTest {
    @Test public void rejectsInitialUrlsOutsideFixedRepoBeforeOpeningNetwork() {
        GithubUpdateTransport transport = new GithubUpdateTransport();
        for (String url : new String[]{"https://example.com/latest.json", "https://github.com/other/wallta/releases/latest/download/latest.json", "http://github.com/mawta/wallta/releases/latest/download/latest.json", "https://objects.githubusercontent.com/arbitrary"})
            assertThrows(IllegalArgumentException.class, () -> transport.open(url));
    }
}
