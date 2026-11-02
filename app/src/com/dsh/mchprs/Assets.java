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
 * 覆盖策略：
 * - docs/ 与 python/core/：每次启动覆盖，跟着版本更新；
 * - python 顶层脚本（correctness.py / timing.py / pipeline.py / README.md）：只补缺，
 *   因为这几个文件顶部是用户要改的配置区，覆盖会把它们冲掉。
 */
public final class Assets {

    private Assets() {
    }

    /** 释放全部内置资源，返回写出的文件数。失败只静默跳过，不影响 App 使用。 */
    public static int extract(Context c, String root) {
        AssetManager am = c.getAssets();
        File base = new File(root);
        int n = 0;
        n += tree(am, "docs", new File(base, "docs"), true);
        n += tree(am, "python/core", new File(base, "python" + File.separator + "core"), true);
        n += flat(am, "python", new File(base, "python"), false);
        n += flat(am, "", base, true);
        return n;
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
