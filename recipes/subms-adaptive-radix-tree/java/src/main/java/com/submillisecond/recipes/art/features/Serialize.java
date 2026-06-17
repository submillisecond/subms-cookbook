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
 * <p>Byte-equivalent on disk to the Rust sibling
 * {@code subms_adaptive_radix_tree::features::serialize}:
 *
 * <pre>
 * magic:        b"ARTb"                 (4 bytes)
 * version:      u16                     (= 1)
 * reserved:     u16                     (= 0)
 * len:          u64                     (entries with values)
 * node-stream:  pre-order
 * </pre>
 *
 * <p>Each node is one of: empty / value-only / children-only / both.
 * Values are serialised through a user-supplied {@link Codec}. Codecs
 * ship for {@code byte[]}, {@code String}, {@code Integer},
 * {@code Long}.
 */
public final class Serialize {

    private static final byte[] MAGIC = {'A', 'R', 'T', 'b'};
    private static final short VERSION = 1;

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

        List<byte[]> orderedKeys = new ArrayList<>();
        List<V> orderedValues = new ArrayList<>();
        ArtInternals.collect(tree, orderedKeys, orderedValues);
        NodeWriter<V> root = NodeWriter.build(orderedKeys, orderedValues);
        root.write(out, codec);
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

        Art<V> tree = new Art<>();
        readNodeRec(in, codec, new ByteArrayOutputStream(), tree);
        if (tree.size() != expectedLen) {
            throw new IOException("len mismatch: header " + expectedLen + " vs reconstructed " + tree.size());
        }
        return tree;
    }

    private static <V> void readNodeRec(
            DataInputStream in,
            Codec<V> codec,
            ByteArrayOutputStream pathSoFar,
            Art<V> tree) throws IOException {
        int tag = in.readUnsignedByte();
        if (tag != TAG_EMPTY && tag != TAG_VALUE && tag != TAG_CHILDREN && tag != TAG_BOTH) {
            throw new IOException("bad tag " + tag);
        }
        if (tag == TAG_VALUE || tag == TAG_BOTH) {
            int vlen = in.readInt();
            if (vlen < 0) throw new IOException("negative value length");
            V value = codec.read(in, vlen);
            tree.insert(pathSoFar.toByteArray(), value);
        }
        if (tag == TAG_CHILDREN || tag == TAG_BOTH) {
            int count = in.readUnsignedShort();
            for (int i = 0; i < count; i++) {
                int childByte = in.readUnsignedByte();
                int beforeLen = pathSoFar.size();
                pathSoFar.write(childByte);
                readNodeRec(in, codec, pathSoFar, tree);
                byte[] saved = pathSoFar.toByteArray();
                pathSoFar.reset();
                pathSoFar.write(saved, 0, beforeLen);
            }
        }
    }

    /** Internal builder mirroring the tree's pre-order shape for a write
     *  pass. Keeps `Art` itself free of serialization knobs. */
    private static final class NodeWriter<V> {
        V value;
        TreeMap<Byte, NodeWriter<V>> children = new TreeMap<>();

        static <V> NodeWriter<V> build(List<byte[]> keys, List<V> values) {
            NodeWriter<V> root = new NodeWriter<>();
            for (int i = 0; i < keys.size(); i++) {
                byte[] k = keys.get(i);
                NodeWriter<V> cur = root;
                for (byte b : k) {
                    cur = cur.children.computeIfAbsent(b, x -> new NodeWriter<>());
                }
                cur.value = values.get(i);
            }
            return root;
        }

        void write(DataOutputStream out, Codec<V> codec) throws IOException {
            boolean hasValue = value != null;
            boolean hasChildren = !children.isEmpty();
            byte tag;
            if (!hasValue && !hasChildren) tag = TAG_EMPTY;
            else if (hasValue && !hasChildren) tag = TAG_VALUE;
            else if (!hasValue) tag = TAG_CHILDREN;
            else tag = TAG_BOTH;
            out.writeByte(tag);
            if (hasValue) {
                ByteArrayOutputStream tmp = new ByteArrayOutputStream();
                codec.write(value, new DataOutputStream(tmp));
                out.writeInt(tmp.size());
                tmp.writeTo(out);
            }
            if (hasChildren) {
                out.writeShort(children.size());
                for (Map.Entry<Byte, NodeWriter<V>> e : children.entrySet()) {
                    out.writeByte(e.getKey());
                    e.getValue().write(out, codec);
                }
            }
        }
    }
}
