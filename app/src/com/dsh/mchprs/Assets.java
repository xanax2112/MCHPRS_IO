package com.dsh.mchprs;

import android.content.Context;
import android.content.res.AssetManager;

import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

/**
 * 把打进 APK 的文档与 Python 工具释放到工作目录（用户 2026-11-02 需求）。
 *
 * 释放策略（用户 2026-11-05 要求：**只在第一次启动时解压一次，之后不管**）：
 * - 工作目录里没有 {@link #MARKER} 时做一次释放；
 * - 释放完写下标记文件，以后启动直接跳过，工作目录里的文件怎么改都不再碰；
 * - 想重新释放一次：删掉工作目录里的 `.apk-assets-done` 再启动 App；
 * - 释放时仍然只补缺（目标已存在就跳过），避免把用户改过的文件冲掉。
 */
public final class Assets {

    private Assets() {
    }

    /** 标记文件：它在工作目录里就表示内置资源已经释放过，不再重复解压。 */
    private static final String MARKER = ".apk-assets-done";

    /** 写进标记文件的内容，只用于排查是哪一版释放的。 */
    private static final String BUNDLE_VERSION = "0.6.2-beta";

    /** 释放全部内置资源，返回写出的文件数。已经释放过（有标记）时直接返回 0。 */
    public static int extract(Context c, String root) {
        File base = new File(root);
        File marker = new File(base, MARKER);
        if (marker.isFile()) {
            return 0;
        }
        AssetManager am = c.getAssets();
        int n = 0;
        n += tree(am, "docs", new File(base, "docs"), false);
        n += tree(am, "python/core", new File(base, "python" + File.separator + "core"), false);
        n += flat(am, "python", new File(base, "python"), false);
        n += flat(am, "", base, true);
        writeMarker(marker);
        return n;
    }

    /** 写下"已释放"标记；失败也不影响 App 使用，只是下次启动会再扫一遍。 */
    private static void writeMarker(File marker) {
        OutputStream os = null;
        try {
            File dir = marker.getParentFile();
            if (dir != null && !dir.isDirectory() && !dir.mkdirs()) {
                return;
            }
            os = new FileOutputStream(marker);
            os.write(BUNDLE_VERSION.getBytes("UTF-8"));
            os.flush();
        } catch (Exception ignored) {
        } finally {
            close(os);
        }
    }

    /** 递归复制一个 assets 目录。 */
    private static int tree(AssetManager am, String path, File dst, boolean overwrite) {
        String[] names = listAll(am, path);
        if (names == null) {
            return 0;
        }
        if (!dst.isDirectory() && !dst.mkdirs()) {
            return 0;
        }
        int n = 0;
        for (String name : names) {
            String child = path + "/" + name;
            File out = new File(dst, name);
            if (isFile(am, child)) {
                n += write(am, child, out, overwrite);
            } else {
                n += tree(am, child, out, overwrite);
            }
        }
        return n;
    }

    /** 只复制该层目录下的 .md / .py 文件（不递归），用于 python 顶层与工作目录根。 */
    private static int flat(AssetManager am, String path, File dst, boolean overwrite) {
        String[] names = listAll(am, path);
        if (names == null) {
            return 0;
        }
        int n = 0;
        for (String name : names) {
            if (!name.endsWith(".md") && !name.endsWith(".py")) {
                continue;
            }
            String asset = path.isEmpty() ? name : path + "/" + name;
            n += write(am, asset, new File(dst, name), overwrite);
        }
        return n;
    }

    private static String[] listAll(AssetManager am, String path) {
        try {
            return am.list(path);
        } catch (Exception e) {
            return null;
        }
    }

    private static boolean isFile(AssetManager am, String path) {
        InputStream in = null;
        try {
            in = am.open(path);
            return true;
        } catch (Exception e) {
            return false;
        } finally {
            close(in);
        }
    }

    private static int write(AssetManager am, String asset, File out, boolean overwrite) {
        if (!overwrite && out.exists()) {
            return 0;
        }
        InputStream in = null;
        OutputStream os = null;
        try {
            File dir = out.getParentFile();
            if (dir != null && !dir.isDirectory() && !dir.mkdirs()) {
                return 0;
            }
            in = am.open(asset);
            os = new FileOutputStream(out);
            byte[] buf = new byte[8192];
            int r;
            while ((r = in.read(buf)) > 0) {
                os.write(buf, 0, r);
            }
            os.flush();
            return 1;
        } catch (Exception e) {
            return 0;
        } finally {
            close(in);
            close(os);
        }
    }

    private static void close(Object o) {
        try {
            if (o instanceof InputStream) {
                ((InputStream) o).close();
            } else if (o instanceof OutputStream) {
                ((OutputStream) o).close();
            }
        } catch (Exception ignored) {
        }
    }
}
