package io.ixmati.javalive;

import org.junit.jupiter.api.Test;

import java.nio.file.Files;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

class LiveClientTest {
    @Test
    void parsesIndependentClientConfiguration() {
        LiveClient.Config config = LiveClient.Config.parse(new String[] {
                "--mode", "ixmati", "--client-id", "3", "--write-rate", "7", "--read-rate", "11", "--duration", "13"
        });
        assertEquals("ixmati", config.mode);
        assertEquals("3", config.clientId);
        assertEquals(7, config.writeRate);
        assertEquals(11, config.readRate);
        assertEquals(13, config.durationSeconds);
    }

    @Test
    void directStoreUsesDurableIdempotentSchema() throws Exception {
        var database = Files.createTempFile("java-live-", ".sqlite");
        LiveClient.DirectStore.initialize(database);
        LiveClient.DirectStore.write(database, "k-1", "idem-1", "test");
        assertTrue(LiveClient.DirectStore.read(database, "k-1"));
        LiveClient.DirectStore.write(database, "k-1", "idem-1", "test");
        try (var connection = java.sql.DriverManager.getConnection("jdbc:sqlite:" + database);
             var statement = connection.createStatement();
             var rows = statement.executeQuery("SELECT COUNT(*) FROM _idempotency")) {
            assertTrue(rows.next());
            assertEquals(1, rows.getInt(1));
        }
        Files.deleteIfExists(database);
        Files.deleteIfExists(database.resolveSibling(database.getFileName() + "-wal"));
        Files.deleteIfExists(database.resolveSibling(database.getFileName() + "-shm"));
    }
}
