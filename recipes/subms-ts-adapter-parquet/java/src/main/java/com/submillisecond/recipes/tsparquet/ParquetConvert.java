package com.submillisecond.recipes.tsparquet;

import com.submillisecond.recipes.ts.TsCollection;
import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesMetadata;
import java.io.ByteArrayOutputStream;
import java.io.EOFException;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import org.apache.hadoop.conf.Configuration;
import org.apache.parquet.HadoopReadOptions;
import org.apache.parquet.ParquetReadOptions;
import org.apache.parquet.column.page.PageReadStore;
import org.apache.parquet.example.data.Group;
import org.apache.parquet.example.data.simple.convert.GroupRecordConverter;
import org.apache.parquet.hadoop.ParquetFileReader;
import org.apache.parquet.hadoop.ParquetWriter;
import org.apache.parquet.io.ColumnIOFactory;
import org.apache.parquet.io.InputFile;
import org.apache.parquet.io.MessageColumnIO;
import org.apache.parquet.io.OutputFile;
import org.apache.parquet.io.PositionOutputStream;
import org.apache.parquet.io.RecordReader;
import org.apache.parquet.io.SeekableInputStream;
import org.apache.parquet.schema.MessageType;
import org.apache.parquet.schema.PrimitiveType;
import org.apache.parquet.schema.Types;

/**
 * The mapping core: {@link TsSeriesD} / {@link TsCollection} &lt;-&gt; Apache
 * Parquet bytes via parquet-mr's Group API.
 *
 * <p>parquet-mr has no clean Arrow bridge, so the Java port maps directly to a
 * Parquet {@code MessageType} (the Rust port composes on subms-ts-adapter-arrow instead);
 * the on-disk Parquet file both produce is the shared interop surface. Series
 * identity rides in the file's key-value metadata. Reads and writes go through
 * fully in-memory {@code OutputFile} / {@code InputFile} backed by a byte array -
 * no filesystem, no Hadoop FileSystem, so the hot path pays no disk latency and
 * no winutils / native libraries are touched.
 */
public final class ParquetConvert {

    private ParquetConvert() {}

    static {
        quietParquetDebugLogging();
    }

    /**
     * parquet-mr's {@code MessageColumnIO} formats an slf4j debug string for every
     * field of every row it writes - but only when its logger is at DEBUG, which
     * it captures into a {@code static final} flag at class load. Hadoop drags in
     * reload4j, whose root logger defaults to DEBUG when unconfigured, so that
     * flag comes up true and a 256-row encode allocates ~3.7 MB of throwaway log
     * strings (vs ~170 KB without) - which is the entire source of the encode p99
     * GC tail. Raise the parquet logger to WARN before that class loads. Done by
     * reflection so it is a harmless no-op under a non-log4j backend; a consumer
     * who has genuinely configured parquet at DEBUG keeps their setting.
     */
    private static void quietParquetDebugLogging() {
        try {
            Class<?> level = Class.forName("org.apache.log4j.Level");
            Object warn = level.getField("WARN").get(null);
            Class<?> logger = Class.forName("org.apache.log4j.Logger");
            Object parquetLogger = logger.getMethod("getLogger", String.class).invoke(null, "org.apache.parquet");
            if (((Boolean) logger.getMethod("isDebugEnabled").invoke(parquetLogger))) {
                logger.getMethod("setLevel", level).invoke(parquetLogger, warn);
            }
        } catch (Throwable ignored) {
            // Not a log4j/reload4j backend - the consumer owns logging config.
        }
    }

    private static final String MK_SID = "subms.sid";
    private static final String MK_NAME = "subms.name";
    private static final String TAG_PREFIX = "subms.tag.";
    private static final String NAME_PREFIX = "subms.name.";

    private static final MessageType SERIES_SCHEMA = Types.buildMessage()
            .required(PrimitiveType.PrimitiveTypeName.INT64).named("ts")
            .required(PrimitiveType.PrimitiveTypeName.DOUBLE).named("v")
            .named("ts_series");

    private static final MessageType COLLECTION_SCHEMA = Types.buildMessage()
            .required(PrimitiveType.PrimitiveTypeName.INT64).named("sid")
            .required(PrimitiveType.PrimitiveTypeName.INT64).named("ts")
            .required(PrimitiveType.PrimitiveTypeName.DOUBLE).named("v")
            .named("ts_collection");

    private static Map<String, String> metaToMap(TsSeriesMetadata meta) {
        Map<String, String> md = new HashMap<>();
        md.put(MK_SID, Long.toString(meta.id()));
        md.put(MK_NAME, meta.name());
        for (Map.Entry<String, String> e : meta.tags().entrySet()) {
            md.put(TAG_PREFIX + e.getKey(), e.getValue());
        }
        return md;
    }

    private static TsSeriesMetadata mapToMeta(Map<String, String> md) {
        String sidStr = md.get(MK_SID);
        if (sidStr == null) {
            return null;
        }
        long sid;
        try {
            sid = Long.parseLong(sidStr);
        } catch (NumberFormatException e) {
            return null;
        }
        TsSeriesMetadata meta = new TsSeriesMetadata(sid, md.getOrDefault(MK_NAME, ""));
        for (Map.Entry<String, String> e : md.entrySet()) {
            if (e.getKey().startsWith(TAG_PREFIX)) {
                meta = meta.withTag(e.getKey().substring(TAG_PREFIX.length()), e.getValue());
            }
        }
        return meta;
    }

    /** Persist one series to Parquet bytes with page sizing derived from the data. */
    public static byte[] seriesToParquet(TsSeriesD series) {
        return seriesToParquet(series, TsParquetWriteOptions.forPointCount(series.size()));
    }

    /** Persist one series to Parquet bytes with explicit writer sizing. */
    public static byte[] seriesToParquet(TsSeriesD series, TsParquetWriteOptions opts) {
        Map<String, String> extra =
                series.metadata().map(ParquetConvert::metaToMap).orElseGet(HashMap::new);
        List<TsPoint<Double>> pts = series.toList();
        return writeRecords(SERIES_SCHEMA, extra, opts, w -> {
            long[] h = new long[2];
            for (TsPoint<Double> p : pts) {
                h[0] = p.ts();
                h[1] = Double.doubleToLongBits(p.value());
                w.write(h);
            }
        });
    }

    /** Read a series back from Parquet bytes. */
    public static TsSeriesD parquetToSeries(byte[] bytes) {
        Map<String, String> kv = new HashMap<>();
        List<Group> groups = readGroups(bytes, kv);
        TsSeriesD series = TsSeriesD.withCapacity(groups.size());
        TsSeriesMetadata meta = mapToMeta(kv);
        if (meta != null) {
            series.setMetadata(meta);
        }
        long[][] rows = new long[groups.size()][];
        for (int i = 0; i < groups.size(); i++) {
            Group g = groups.get(i);
            rows[i] = new long[] {g.getLong("ts", 0), Double.doubleToLongBits(g.getDouble("v", 0))};
        }
        java.util.Arrays.sort(rows, (a, b) -> Long.compare(a[0], b[0]));
        for (long[] r : rows) {
            series.push(r[0], Double.longBitsToDouble(r[1]));
        }
        return series;
    }

    /** Persist a collection to Parquet bytes (the long-format sid/ts/v). */
    public static byte[] collectionToParquet(TsCollection<Double> coll) {
        long points = 0;
        for (TsSeries<Double> s : coll.series()) {
            points += s.size();
        }
        return collectionToParquet(coll, TsParquetWriteOptions.forPointCount(points));
    }

    /** Persist a collection to Parquet bytes with explicit writer sizing. */
    public static byte[] collectionToParquet(TsCollection<Double> coll, TsParquetWriteOptions opts) {
        Map<String, String> extra = new HashMap<>();
        for (TsSeries<Double> s : coll.series()) {
            s.metadata().ifPresent(m -> extra.put(NAME_PREFIX + m.id(), m.name()));
        }
        return writeRecords(COLLECTION_SCHEMA, extra, opts, w -> {
            long[] h = new long[3];
            for (TsSeries<Double> s : coll.series()) {
                long sid = s.metadata().map(TsSeriesMetadata::id).orElse(0L);
                for (TsPoint<Double> p : s) {
                    h[0] = sid;
                    h[1] = p.ts();
                    h[2] = Double.doubleToLongBits(p.value());
                    w.write(h);
                }
            }
        });
    }

    /** Read a collection back from Parquet bytes. */
    public static TsCollection<Double> parquetToCollection(byte[] bytes) {
        Map<String, String> kv = new HashMap<>();
        List<Group> groups = readGroups(bytes, kv);
        long[][] rows = new long[groups.size()][];
        for (int i = 0; i < groups.size(); i++) {
            Group g = groups.get(i);
            rows[i] = new long[] {g.getLong("sid", 0), g.getLong("ts", 0), Double.doubleToLongBits(g.getDouble("v", 0))};
        }
        java.util.Arrays.sort(rows, (a, b) -> a[0] != b[0] ? Long.compare(a[0], b[0]) : Long.compare(a[1], b[1]));
        TsCollection<Double> coll = new TsCollection<>();
        long current = Long.MIN_VALUE;
        boolean any = false;
        for (long[] r : rows) {
            if (!any || r[0] != current) {
                coll.register(new TsSeriesMetadata(r[0], kv.getOrDefault(NAME_PREFIX + r[0], "")));
                current = r[0];
                any = true;
            }
            coll.push(r[0], r[1], Double.longBitsToDouble(r[2]));
        }
        return coll;
    }

    // An empty configuration (no default resources parsed) reused across calls.
    // parquet-mr only reads the write schema property off it for in-memory use,
    // so loading core-default.xml per op would be pure waste.
    private static final Configuration CONF = new Configuration(false);

    // Pre-built read options over the same empty config. The single-arg
    // ParquetFileReader.open(InputFile) constructs a fresh Configuration (an XML
    // parse, ~2 ms) on every call; passing explicit options skips it entirely.
    private static final ParquetReadOptions READ_OPTS = HadoopReadOptions.builder(CONF).build();

    @FunctionalInterface
    private interface RowSource {
        void writeTo(ParquetWriter<long[]> w) throws IOException;
    }

    // Writes rows through a direct WriteSupport over a single reused long[] holder
    // (the value column is carried as raw bits). This avoids allocating a Group
    // object per row, which - at hundreds of rows per file - is the dominant
    // per-op garbage and the driver of the GC pauses in the p99 tail.
    private static byte[] writeRecords(
            MessageType schema, Map<String, String> extra, TsParquetWriteOptions opts, RowSource src) {
        try {
            ByteArrayOutputFile out = new ByteArrayOutputFile();
            try (ParquetWriter<long[]> w = new DirectBuilder(out, schema, extra)
                    .withConf(CONF)
                    .withPageSize(opts.pageSize())
                    .withDictionaryPageSize(opts.pageSize())
                    .withRowGroupSize(opts.rowGroupSize())
                    .withDictionaryEncoding(opts.dictionary())
                    .build()) {
                src.writeTo(w);
            }
            return out.toByteArray();
        } catch (IOException e) {
            throw TsParquetException.parquet(String.valueOf(e.getMessage()));
        }
    }

    /** Writes a row of int64 / double columns straight to the record consumer. */
    static final class DirectWriteSupport extends org.apache.parquet.hadoop.api.WriteSupport<long[]> {
        private final MessageType schema;
        private final Map<String, String> extra;
        private final String[] names;
        private final boolean[] isLong;
        private org.apache.parquet.io.api.RecordConsumer rc;

        DirectWriteSupport(MessageType schema, Map<String, String> extra) {
            this.schema = schema;
            this.extra = extra;
            int n = schema.getFieldCount();
            names = new String[n];
            isLong = new boolean[n];
            for (int i = 0; i < n; i++) {
                names[i] = schema.getType(i).getName();
                isLong[i] = schema.getType(i).asPrimitiveType().getPrimitiveTypeName()
                        == PrimitiveType.PrimitiveTypeName.INT64;
            }
        }

        @Override
        public WriteContext init(Configuration configuration) {
            return new WriteContext(schema, extra);
        }

        @Override
        public void prepareForWrite(org.apache.parquet.io.api.RecordConsumer recordConsumer) {
            this.rc = recordConsumer;
        }

        @Override
        public void write(long[] rec) {
            rc.startMessage();
            for (int i = 0; i < names.length; i++) {
                rc.startField(names[i], i);
                if (isLong[i]) {
                    rc.addLong(rec[i]);
                } else {
                    rc.addDouble(Double.longBitsToDouble(rec[i]));
                }
                rc.endField(names[i], i);
            }
            rc.endMessage();
        }
    }

    static final class DirectBuilder extends ParquetWriter.Builder<long[], DirectBuilder> {
        private final org.apache.parquet.hadoop.api.WriteSupport<long[]> ws;

        DirectBuilder(OutputFile file, MessageType schema, Map<String, String> extra) {
            super(file);
            this.ws = new DirectWriteSupport(schema, extra);
        }

        @Override
        protected DirectBuilder self() {
            return this;
        }

        @Override
        protected org.apache.parquet.hadoop.api.WriteSupport<long[]> getWriteSupport(Configuration conf) {
            return ws;
        }
    }

    private static List<Group> readGroups(byte[] bytes, Map<String, String> kvOut) {
        try {
            InputFile in = new ByteArrayInputFile(bytes);
            List<Group> groups = new ArrayList<>();
            try (ParquetFileReader fr = ParquetFileReader.open(in, READ_OPTS)) {
                kvOut.putAll(fr.getFooter().getFileMetaData().getKeyValueMetaData());
                MessageType schema = fr.getFooter().getFileMetaData().getSchema();
                MessageColumnIO columnIO = new ColumnIOFactory().getColumnIO(schema);
                GroupRecordConverter converter = new GroupRecordConverter(schema);
                PageReadStore pages;
                while ((pages = fr.readNextRowGroup()) != null) {
                    long rows = pages.getRowCount();
                    RecordReader<Group> recordReader = columnIO.getRecordReader(pages, converter);
                    for (long i = 0; i < rows; i++) {
                        groups.add(recordReader.read());
                    }
                }
            }
            return groups;
        } catch (IOException | RuntimeException e) {
            throw TsParquetException.parquet(String.valueOf(e.getMessage()));
        }
    }

    /** A fully in-memory OutputFile - getPos() is the buffer size, always exact. */
    static final class ByteArrayOutputFile implements OutputFile {
        private final ByteArrayOutputStream baos = new ByteArrayOutputStream(1024);

        @Override
        public PositionOutputStream create(long blockSizeHint) {
            return stream();
        }

        @Override
        public PositionOutputStream createOrOverwrite(long blockSizeHint) {
            baos.reset();
            return stream();
        }

        @Override
        public boolean supportsBlockSize() {
            return false;
        }

        @Override
        public long defaultBlockSize() {
            return 0;
        }

        byte[] toByteArray() {
            return baos.toByteArray();
        }

        private PositionOutputStream stream() {
            return new PositionOutputStream() {
                @Override
                public long getPos() {
                    return baos.size();
                }

                @Override
                public void write(int b) {
                    baos.write(b);
                }

                @Override
                public void write(byte[] b, int off, int len) {
                    baos.write(b, off, len);
                }
            };
        }
    }

    /** A fully in-memory InputFile over a byte array. */
    static final class ByteArrayInputFile implements InputFile {
        private final byte[] data;

        ByteArrayInputFile(byte[] data) {
            this.data = data;
        }

        @Override
        public long getLength() {
            return data.length;
        }

        @Override
        public SeekableInputStream newStream() {
            return new ByteArraySeekableInputStream(data);
        }
    }

    static final class ByteArraySeekableInputStream extends SeekableInputStream {
        private final byte[] data;
        private int pos;

        ByteArraySeekableInputStream(byte[] data) {
            this.data = data;
        }

        @Override
        public long getPos() {
            return pos;
        }

        @Override
        public void seek(long newPos) {
            pos = (int) newPos;
        }

        @Override
        public int read() {
            return pos < data.length ? (data[pos++] & 0xff) : -1;
        }

        @Override
        public int read(byte[] b, int off, int len) {
            if (pos >= data.length) {
                return -1;
            }
            int n = Math.min(len, data.length - pos);
            System.arraycopy(data, pos, b, off, n);
            pos += n;
            return n;
        }

        @Override
        public void readFully(byte[] b) throws IOException {
            readFully(b, 0, b.length);
        }

        @Override
        public void readFully(byte[] b, int off, int len) throws IOException {
            if (pos + len > data.length) {
                throw new EOFException();
            }
            System.arraycopy(data, pos, b, off, len);
            pos += len;
        }

        @Override
        public int read(ByteBuffer buf) {
            int n = Math.min(buf.remaining(), data.length - pos);
            if (n <= 0) {
                return -1;
            }
            buf.put(data, pos, n);
            pos += n;
            return n;
        }

        @Override
        public void readFully(ByteBuffer buf) throws IOException {
            int n = buf.remaining();
            if (pos + n > data.length) {
                throw new EOFException();
            }
            buf.put(data, pos, n);
            pos += n;
        }
    }
}
