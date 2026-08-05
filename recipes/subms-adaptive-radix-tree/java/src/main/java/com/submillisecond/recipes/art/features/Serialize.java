package com.submillisecond.recipes.art.features;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;

import com.submillisecond.recipes.art.Art;
import com.submillisecond.recipes.art.ArtInternals;

/**
 * Binary write/read for an ART. Custom format - not Java serialisation.
 * A versioned header followed by a node-tagged pre-order stream.
 *
 * <pre>
 * magic:        b"ARTb"                 (4 bytes)
 * version:      u16                     (= 2)
 * reserved:     u16                     (= 0)
 * len:          u64                     (entries with values)
 * node-stream:  pre-order, one record per TREE NODE
 * </pre>
 *
 * <p>Each node is {@code prefix_len:u16}, {@code prefix}, then a shape tag -
 * empty / value-only / children-only / both - and each child's edge byte is
 * written immediately before that child's whole record.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_adaptive_radix_tree::features::serialize}: a tree written by
 * either port is read by the other, and the bytes are identical for identical
 * input. That is PINNED by a shared hex fixture in both test suites
 * ({@code SerializeCrossPortTest} here, {@code serialize_tests.rs} there) -
 * not asserted, because it was asserted once and was false. This port shipped
 * version 1 (one record per key BYTE) while Rust shipped version 2 (one record
 * per node, carrying its path-compressed prefix); each passed its own
 * round-trip suite, each rejected the other's stream, and this javadoc claimed
 * they matched. A round-trip only proves a port agrees with itself.
 * Values are serialised through a user-supplied {@link Codec}. Codecs
 * ship for {@code byte[]}, {@code String}, {@code Integer},
 * {@code Long}.
 */
public final class Serialize {

    private static final byte[] MAGIC = {'A', 'R', 'T', 'b'};
    private static final short VERSION = 2;

    private static final byte TAG_EMPTY = 0x00;
    private static final byte TAG_VALUE = 0x01;
    private static final byte TAG_CHILDREN = 0x02;
    private static final byte TAG_BOTH = 0x03;

    private Serialize() {}

    /** User-supplied value codec. */
    public interface Codec<V> {
        void write(V value, DataOutputStream out) throws IOException;
        V read(DataInputStream in, int len) throws IOException;
    }

    public static final Codec<byte[]> BYTES = new Codec<>() {
        @Override public void write(byte[] value, DataOutputStream out) throws IOException {
            out.write(value);
        }
        @Override public byte[] read(DataInputStream in, int len) throws IOException {
            byte[] buf = new byte[len];
            in.readFully(buf);
            return buf;
        }
    };

    public static final Codec<String> STRING = new Codec<>() {
        @Override public void write(String value, DataOutputStream out) throws IOException {
            out.write(value.getBytes(StandardCharsets.UTF_8));
        }
        @Override public String read(DataInputStream in, int len) throws IOException {
            byte[] buf = new byte[len];
            in.readFully(buf);
            return new String(buf, StandardCharsets.UTF_8);
        }
    };

    public static final Codec<Integer> INT32 = new Codec<>() {
        @Override public void write(Integer value, DataOutputStream out) throws IOException {
            out.writeInt(value);
        }
        @Override public Integer read(DataInputStream in, int len) throws IOException {
            if (len != 4) throw new IOException("int32 value not 4 bytes: " + len);
            return in.readInt();
        }
    };

    public static final Codec<Long> INT64 = new Codec<>() {
        @Override public void write(Long value, DataOutputStream out) throws IOException {
            out.writeLong(value);
        }
        @Override public Long read(DataInputStream in, int len) throws IOException {
            if (len != 8) throw new IOException("int64 value not 8 bytes: " + len);
            return in.readLong();
        }
    };

    public static <V> byte[] writeToBytes(Art<V> tree, Codec<V> codec) throws IOException {
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        writeTo(tree, codec, out);
        return out.toByteArray();
    }

    public static <V> void writeTo(Art<V> tree, Codec<V> codec, OutputStream raw) throws IOException {
        DataOutputStream out = new DataOutputStream(raw);
        out.write(MAGIC);
        out.writeShort(VERSION);
        out.writeShort(0);
        out.writeLong(tree.size());

        // One record per NODE, carrying its path-compressed prefix, with each
        // child's edge byte interleaved before that child's whole record - the
        // same stream the Rust port writes. v1 emitted one record per key BYTE
        // and rebuilt by re-inserting whole keys, producing a structurally
        // different stream Rust could not read, while this file's javadoc
        // claimed the two were byte-equivalent.
        writeNode(ArtInternals.rootRef(tree), out, codec);
        out.flush();
    }

    public static <V> Art<V> parseBytes(byte[] data, Codec<V> codec) throws IOException {
        return parse(new ByteArrayInputStream(data), codec);
    }

    public static <V> Art<V> parse(InputStream raw, Codec<V> codec) throws IOException {
        DataInputStream in = new DataInputStream(raw);
        byte[] magic = new byte[4];
        in.readFully(magic);
        for (int i = 0; i < 4; i++) {
            if (magic[i] != MAGIC[i]) throw new IOException("bad magic");
        }
        int version = in.readUnsignedShort();
        if (version != VERSION) throw new IOException("unsupported version " + version);
        in.readUnsignedShort();
        long expectedLen = in.readLong();

        if (expectedLen < 0 || expectedLen > Integer.MAX_VALUE) {
            throw new IOException("implausible entry count " + expectedLen);
        }
        Art<V> tree = new Art<>();
        readNode(in, codec, ArtInternals.rootHandle(tree));
        // The header carries the count rather than the reader tallying values,
        // so a truncated stream is a read error and not a silently short tree.
        ArtInternals.setSize(tree, (int) expectedLen);
        return tree;
    }

    private static <V> void writeNode(
            ArtInternals.NodeRef<V> node, DataOutputStream out, Codec<V> codec) throws IOException {
        byte[] prefix = node.prefix();
        out.writeShort(prefix.length);
        out.write(prefix);

        V value = node.value();
        byte[] childBytes = node.childBytes();
        boolean hasValue = value != null;
        boolean hasChildren = childBytes.length > 0;
        int tag = hasValue ? (hasChildren ? TAG_BOTH : TAG_VALUE)
                           : (hasChildren ? TAG_CHILDREN : TAG_EMPTY);
        out.writeByte(tag);

        if (hasValue) {
            // Length-prefixed, so the value bytes must be sized before writing.
            ByteArrayOutputStream vbuf = new ByteArrayOutputStream();
            DataOutputStream vout = new DataOutputStream(vbuf);
            codec.write(value, vout);
            vout.flush();
            byte[] vbytes = vbuf.toByteArray();
            out.writeInt(vbytes.length);
            out.write(vbytes);
        }
        if (hasChildren) {
            out.writeShort(childBytes.length);
            for (byte b : childBytes) {
                out.writeByte(b);
                writeNode(node.child(b), out, codec);
            }
        }
    }

    private static <V> void readNode(
            DataInputStream in, Codec<V> codec, ArtInternals.NodeHandle<V> node) throws IOException {
        int plen = in.readUnsignedShort();
        byte[] prefix = new byte[plen];
        in.readFully(prefix);

        int tag = in.readUnsignedByte();
        if (tag != TAG_EMPTY && tag != TAG_VALUE && tag != TAG_CHILDREN && tag != TAG_BOTH) {
            throw new IOException("bad tag " + tag);
        }

        V value = null;
        if (tag == TAG_VALUE || tag == TAG_BOTH) {
            int vlen = in.readInt();
            if (vlen < 0) throw new IOException("negative value length");
            value = codec.read(in, vlen);
        }
        node.set(prefix, value);

        if (tag == TAG_CHILDREN || tag == TAG_BOTH) {
            int count = in.readUnsignedShort();
            for (int i = 0; i < count; i++) {
                byte edge = (byte) in.readUnsignedByte();
                readNode(in, codec, node.child(edge));
            }
        }
    }

    /** Internal builder mirroring the tree's pre-order shape for a write
     *  pass. Keeps `Art` itself free of serialization knobs. */
}
