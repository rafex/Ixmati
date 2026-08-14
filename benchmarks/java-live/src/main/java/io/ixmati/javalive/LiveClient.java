package io.ixmati.javalive;

import com.google.protobuf.Struct;
import com.google.protobuf.Value;
import io.grpc.ManagedChannel;
import io.grpc.Metadata;
import io.grpc.StatusRuntimeException;
import io.grpc.stub.MetadataUtils;
import io.grpc.netty.shaded.io.grpc.netty.NettyChannelBuilder;
import ixmati.v1.Common;
import ixmati.v1.Read;
import ixmati.v1.ReadServiceGrpc;
import ixmati.v1.Write;
import ixmati.v1.WriteServiceGrpc;
import org.sqlite.SQLiteConfig;

import java.io.BufferedWriter;
import java.io.IOException;
import java.net.URI;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.PreparedStatement;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.sql.Statement;
import java.time.Instant;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicBoolean;

/** One independent workload process; used by one container per Java client. */
public final class LiveClient {
    private final Config config;
    private final Stats stats = new Stats();
    private final AtomicBoolean running = new AtomicBoolean(true);
    private final Path snapshotFile;
    private final IxmatiStore ixmati;

    private LiveClient(Config config) throws IOException {
        this.config = config;
        if (config.mode.equals("direct")) {
            try { DirectStore.initialize(config.dbPath); }
            catch (SQLException e) { throw new IOException("cannot initialize direct SQLite", e); }
        }
        Files.createDirectories(config.snapshotDir);
        snapshotFile = config.snapshotDir.resolve(config.mode + "-" + config.clientId + ".jsonl");
        ixmati = config.mode.equals("ixmati") ? new IxmatiStore(config.endpoint, config.apiKey) : null;
    }

    public static void main(String[] args) throws Exception {
        Config config = Config.parse(args);
        if (config.initOnly) {
            DirectStore.initialize(config.dbPath);
            return;
        }
        new LiveClient(config).run();
    }

    private void run() throws Exception {
        System.out.printf("java-live client=%s mode=%s writes=%d/s reads=%d/s duration=%ds%n",
                config.clientId, config.mode, config.writeRate, config.readRate, config.durationSeconds);
        long deadline = System.nanoTime() + config.durationSeconds * 1_000_000_000L;
        try (SnapshotWriter snapshots = new SnapshotWriter(snapshotFile)) {
            Thread writer = new Thread(() -> rateLoop(config.writeRate, deadline, this::write), "writer");
            Thread reader = new Thread(() -> rateLoop(config.readRate, deadline, this::read), "reader");
            Thread ticker = new Thread(() -> snapshotLoop(snapshots, deadline), "snapshot");
            writer.start(); reader.start(); ticker.start();
            writer.join(); reader.join(); running.set(false); ticker.join();
            snapshots.write(stats.snapshot(config, true));
        } finally {
            if (ixmati != null) ixmati.close();
        }
        System.out.printf("java-live finished client=%s mode=%s%n", config.clientId, config.mode);
    }

    private void rateLoop(int rate, long deadline, Runnable operation) {
        if (rate <= 0) return;
        long interval = 1_000_000_000L / rate;
        long next = System.nanoTime();
        while (running.get() && System.nanoTime() < deadline) {
            operation.run();
            next += interval;
            long sleep = next - System.nanoTime();
            if (sleep > 0) {
                try { Thread.sleep(sleep / 1_000_000L, (int) (sleep % 1_000_000L)); }
                catch (InterruptedException e) { Thread.currentThread().interrupt(); return; }
            }
        }
    }

    private void snapshotLoop(SnapshotWriter snapshots, long deadline) {
        while (running.get() && System.nanoTime() < deadline) {
            try { Thread.sleep(1000); }
            catch (InterruptedException e) { Thread.currentThread().interrupt(); return; }
            try { snapshots.write(stats.snapshot(config, false)); }
            catch (IOException e) { stats.error(); }
        }
    }

    private void write() {
        long started = System.nanoTime();
        String key = "client-" + config.clientId + "-" + stats.nextWriteNumber();
        String idem = config.mode + "-" + config.clientId + "-" + key;
        try {
            if (config.mode.equals("direct")) DirectStore.write(config.dbPath, key, idem, config.clientId);
            else ixmati.write(key, idem, config.clientId);
            stats.writeCommitted(elapsedMillis(started));
        } catch (BusyException e) { stats.busy(elapsedMillis(started)); }
        catch (PendingException e) { stats.pending(elapsedMillis(started)); }
        catch (Exception e) { stats.writeError(elapsedMillis(started), e); }
    }

    private void read() {
        long started = System.nanoTime();
        String key = "client-" + config.clientId + "-" + Math.max(0, stats.currentWriteNumber() - 1);
        try {
            boolean found = config.mode.equals("direct")
                    ? DirectStore.read(config.dbPath, key)
                    : ixmati.read(key);
            stats.read(found, elapsedMillis(started));
        } catch (Exception e) { stats.readError(elapsedMillis(started), e); }
    }

    private static long elapsedMillis(long started) {
        return Math.max(0, (System.nanoTime() - started) / 1_000_000L);
    }

    static final class Config {
        String mode = "direct", clientId = "1", endpoint = "http://api:30100", apiKey = "ix-live-key";
        Path dbPath = Path.of("/direct-data/demo.sqlite"), snapshotDir = Path.of("/snapshots");
        int writeRate = 20, readRate = 20, durationSeconds = 60;
        boolean initOnly;

        static Config parse(String[] args) {
            Config c = new Config();
            Map<String, String> values = new HashMap<>();
            for (int i = 0; i < args.length; i++) {
                if (args[i].equals("--init-only")) { c.initOnly = true; continue; }
                if (args[i].startsWith("--") && i + 1 < args.length) values.put(args[i].substring(2), args[++i]);
            }
            c.mode = values.getOrDefault("mode", c.mode);
            c.clientId = values.getOrDefault("client-id", c.clientId);
            c.endpoint = values.getOrDefault("grpc-endpoint", c.endpoint);
            c.apiKey = values.getOrDefault("api-key", c.apiKey);
            c.dbPath = Path.of(values.getOrDefault("db-path", c.dbPath.toString()));
            c.snapshotDir = Path.of(values.getOrDefault("snapshot-dir", c.snapshotDir.toString()));
            c.writeRate = integer(values, "write-rate", c.writeRate);
            c.readRate = integer(values, "read-rate", c.readRate);
            c.durationSeconds = integer(values, "duration", c.durationSeconds);
            if (!c.mode.equals("direct") && !c.mode.equals("ixmati")) throw new IllegalArgumentException("mode must be direct or ixmati");
            return c;
        }

        private static int integer(Map<String, String> values, String key, int fallback) {
            try { return Integer.parseInt(values.getOrDefault(key, Integer.toString(fallback))); }
            catch (NumberFormatException e) { throw new IllegalArgumentException("invalid " + key); }
        }
    }

    private static final class SnapshotWriter implements AutoCloseable {
        private final BufferedWriter writer;
        SnapshotWriter(Path file) throws IOException { writer = Files.newBufferedWriter(file, StandardCharsets.UTF_8); }
        synchronized void write(String json) throws IOException { writer.write(json); writer.newLine(); writer.flush(); }
        public void close() throws IOException { writer.close(); }
    }

    private static final class Stats {
        private long writesSent, writesCommitted, pending, writeErrors, reads, readHits, readErrors, busy;
        private long totalWrites, totalCommitted, totalPending, totalWriteErrors, totalReads, totalReadHits, totalReadErrors, totalBusy;
        private long writeNumber;
        private String lastWriteError = "", lastReadError = "";
        private final List<Long> writeLatencies = new ArrayList<>();
        private final List<Long> readLatencies = new ArrayList<>();

        synchronized long nextWriteNumber() { return writeNumber++; }
        synchronized long currentWriteNumber() { return writeNumber; }
        synchronized void writeCommitted(long ms) { writesSent++; writesCommitted++; totalWrites++; totalCommitted++; writeLatencies.add(ms); }
        synchronized void pending(long ms) { writesSent++; pending++; totalWrites++; totalPending++; writeLatencies.add(ms); }
        synchronized void writeError(long ms, Throwable error) {
            writesSent++; writeErrors++; totalWrites++; totalWriteErrors++; writeLatencies.add(ms);
            if (lastWriteError.isEmpty()) lastWriteError = error.toString();
        }
        synchronized void busy(long ms) { writesSent++; busy++; totalWrites++; totalBusy++; writeLatencies.add(ms); }
        synchronized void read(boolean hit, long ms) { reads++; totalReads++; if (hit) { readHits++; totalReadHits++; } readLatencies.add(ms); }
        synchronized void readError(long ms, Throwable error) {
            reads++; readErrors++; totalReads++; totalReadErrors++; readLatencies.add(ms);
            if (lastReadError.isEmpty()) lastReadError = error.toString();
        }
        synchronized void error() { totalWriteErrors++; }

        synchronized String snapshot(Config c, boolean finalSnapshot) {
            String json = "{"
                    + "\"ts\":\"" + Instant.now() + "\",\"mode\":\"" + escape(c.mode) + "\","
                    + "\"client_id\":\"" + escape(c.clientId) + "\",\"final\":" + finalSnapshot + ","
                    + "\"writes_sent\":" + writesSent + ",\"writes_committed\":" + writesCommitted + ","
                    + "\"pending\":" + pending + ",\"write_errors\":" + writeErrors + ",\"sqlite_busy\":" + busy + ","
                    + "\"reads\":" + reads + ",\"read_hits\":" + readHits + ",\"read_errors\":" + readErrors + ","
                    + "\"last_write_error\":\"" + escape(lastWriteError) + "\",\"last_read_error\":\"" + escape(lastReadError) + "\","
                    + "\"p50_ms\":" + percentile(writeLatencies, .50) + ",\"p95_ms\":" + percentile(writeLatencies, .95) + ",\"p99_ms\":" + percentile(writeLatencies, .99) + ","
                    + "\"read_p50_ms\":" + percentile(readLatencies, .50) + ",\"read_p95_ms\":" + percentile(readLatencies, .95) + ","
                    + "\"total_writes\":" + totalWrites + ",\"total_committed\":" + totalCommitted + ",\"total_pending\":" + totalPending + ","
                    + "\"total_write_errors\":" + totalWriteErrors + ",\"total_reads\":" + totalReads + ",\"total_read_hits\":" + totalReadHits + ","
                    + "\"total_read_errors\":" + totalReadErrors + ",\"total_sqlite_busy\":" + totalBusy + "}";
            writesSent = writesCommitted = pending = writeErrors = reads = readHits = readErrors = busy = 0;
            writeLatencies.clear(); readLatencies.clear();
            return json;
        }

        private static long percentile(List<Long> values, double p) {
            if (values.isEmpty()) return 0;
            List<Long> copy = new ArrayList<>(values); Collections.sort(copy);
            return copy.get(Math.min(copy.size() - 1, (int) Math.ceil(p * copy.size()) - 1));
        }
    }

    static final class DirectStore {
        static void initialize(Path path) throws SQLException, IOException {
            if (path.getParent() != null) Files.createDirectories(path.getParent());
            try (Connection c = open(path); Statement s = c.createStatement()) {
                s.execute("PRAGMA journal_mode=WAL");
                s.execute("PRAGMA synchronous=NORMAL");
                s.execute("PRAGMA busy_timeout=5000");
                s.execute("CREATE TABLE IF NOT EXISTS payload_pedidos (entity TEXT NOT NULL,key TEXT NOT NULL,version INTEGER NOT NULL,payload BLOB NOT NULL,updated_at TEXT NOT NULL DEFAULT (datetime('now')),PRIMARY KEY(entity,key))");
                s.execute("CREATE TABLE IF NOT EXISTS _idempotency (idempotency_key TEXT PRIMARY KEY,store TEXT NOT NULL,entity TEXT NOT NULL,key TEXT NOT NULL,version INTEGER NOT NULL,applied_at TEXT NOT NULL DEFAULT (datetime('now')))");
                s.execute("CREATE TABLE IF NOT EXISTS _outbox (id INTEGER PRIMARY KEY AUTOINCREMENT,event_id TEXT NOT NULL UNIQUE,event_type TEXT NOT NULL,store TEXT NOT NULL,entity TEXT NOT NULL,key TEXT NOT NULL,version INTEGER NOT NULL,payload BLOB NOT NULL,published_at TEXT)");
                s.execute("CREATE INDEX IF NOT EXISTS idx_outbox_published ON _outbox(published_at)");
            }
        }

        static void write(Path path, String key, String idem, String client) throws Exception {
            try (Connection c = open(path)) {
                c.setAutoCommit(false);
                try (PreparedStatement p = c.prepareStatement(
                        "INSERT OR IGNORE INTO _idempotency(idempotency_key,store,entity,key,version) VALUES(?,?,?,?,1)")) {
                    p.setString(1, idem); p.setString(2, "default"); p.setString(3, "pedido"); p.setString(4, key);
                    if (p.executeUpdate() == 0) { c.rollback(); return; }
                }
                byte[] payload = ("{\"client_id\":\"" + client + "\",\"key\":\"" + key + "\",\"kind\":\"direct\"}")
                        .getBytes(StandardCharsets.UTF_8);
                try (PreparedStatement p = c.prepareStatement(
                        "INSERT OR REPLACE INTO payload_pedidos(entity,key,version,payload) VALUES('pedido',?,1,?)")) {
                    p.setString(1, key); p.setBytes(2, payload); p.executeUpdate();
                }
                try (PreparedStatement p = c.prepareStatement(
                        "INSERT INTO _outbox(event_id,event_type,store,entity,key,version,payload) VALUES(?,?,?,?,?,1,?)")) {
                    p.setString(1, UUID.randomUUID().toString()); p.setString(2, "upsert");
                    p.setString(3, "default"); p.setString(4, "pedido"); p.setString(5, key); p.setBytes(6, payload);
                    p.executeUpdate();
                }
                c.commit();
            } catch (SQLException e) {
                if (e.getMessage() != null && (e.getMessage().toLowerCase().contains("busy") || e.getMessage().toLowerCase().contains("locked"))) throw new BusyException();
                throw e;
            }
        }

        static boolean read(Path path, String key) throws Exception {
            try (Connection c = open(path);
                 PreparedStatement p = c.prepareStatement("SELECT 1 FROM payload_pedidos WHERE entity='pedido' AND key=?")) {
                p.setString(1, key);
                try (ResultSet r = p.executeQuery()) { return r.next(); }
            }
        }

        private static Connection open(Path path) throws SQLException {
            SQLiteConfig cfg = new SQLiteConfig();
            cfg.setJournalMode(SQLiteConfig.JournalMode.WAL);
            cfg.setSynchronous(SQLiteConfig.SynchronousMode.NORMAL);
            cfg.setBusyTimeout(5000);
            return DriverManager.getConnection("jdbc:sqlite:" + path, cfg.toProperties());
        }
    }

    private static final class IxmatiStore implements AutoCloseable {
        private final ManagedChannel channel;
        private final WriteServiceGrpc.WriteServiceBlockingStub writes;
        private final ReadServiceGrpc.ReadServiceBlockingStub reads;

        IxmatiStore(String endpoint, String apiKey) {
            URI address = URI.create(endpoint.contains("://") ? endpoint : "http://" + endpoint);
            if (address.getHost() == null || address.getPort() < 1) {
                throw new IllegalArgumentException("gRPC endpoint must contain a host and port: " + endpoint);
            }
            // Build a direct TCP channel so a Compose/Podman DNS name is not
            // interpreted as a resolver scheme (for example, "api:30100"
            // may otherwise be treated as a unix target).
            channel = NettyChannelBuilder.forAddress(
                            new InetSocketAddress(address.getHost(), address.getPort()))
                    .usePlaintext().build();
            Metadata headers = new Metadata();
            headers.put(Metadata.Key.of("x-api-key", Metadata.ASCII_STRING_MARSHALLER), apiKey);
            writes = WriteServiceGrpc.newBlockingStub(channel)
                    .withInterceptors(MetadataUtils.newAttachHeadersInterceptor(headers));
            reads = ReadServiceGrpc.newBlockingStub(channel)
                    .withInterceptors(MetadataUtils.newAttachHeadersInterceptor(headers));
        }

        void write(String key, String idem, String client) {
            Struct payload = Struct.newBuilder()
                    .putFields("client_id", Value.newBuilder().setStringValue(client).build())
                    .putFields("key", Value.newBuilder().setStringValue(key).build())
                    .putFields("kind", Value.newBuilder().setStringValue("ixmati").build())
                    .build();
            try {
                Write.WriteResponse response = writes.write(Write.WriteRequest.newBuilder()
                        .setEnvelope(Common.WriteEnvelope.newBuilder()
                                .setOp("upsert").setStore("default").setEntity("pedido").setKey(key)
                                .setVersion(1).setTs(Instant.now().toString()).setIdempotencyKey(idem)
                                .setAckMode("committed").setPayload(payload).build())
                        .build());
                if (response.getStatus().equals("PENDING")) throw new PendingException();
                if (!response.getStatus().equals("COMMITTED") && !response.getStatus().equals("DUPLICATE")) {
                    throw new IllegalStateException(response.getStatus());
                }
            } catch (StatusRuntimeException e) {
                if (e.getStatus().getCode() == io.grpc.Status.Code.RESOURCE_EXHAUSTED) throw new PendingException();
                throw e;
            }
        }

        boolean read(String key) {
            return reads.read(Read.ReadRequest.newBuilder().setStore("default").setEntity("pedido").setKey(key).build()).getFound();
        }

        public void close() { channel.shutdownNow(); }
    }

    private static String escape(String value) {
        return value.replace("\\", "\\\\").replace("\"", "\\\"");
    }

    private static class BusyException extends RuntimeException {}
    private static class PendingException extends RuntimeException {}
}
