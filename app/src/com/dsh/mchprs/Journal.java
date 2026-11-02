package com.dsh.mchprs;

import java.io.File;
import java.io.FileOutputStream;
import java.text.SimpleDateFormat;
import java.util.Date;
import java.util.Locale;
import java.util.regex.Pattern;

/**
 * 落盘日志：写到共享存储，App 被杀后仍可被外部读取。
 * 同时负责「异常终止检测」的 flag 文件。
 */
public final class Journal {

    private static final Pattern ANSI = Pattern.compile("\u001B\\[[;\\d]*[A-Za-z]");
    private static File logDir;

    private Journal() {
    }

    public static synchronized void init(String workdir) {
        if (logDir == null) {
            File d = new File(workdir, "logs");
            if (!d.isDirectory()) {
                d.mkdirs();
            }
            logDir = d;
        }
    }

    public static String stamp() {
        return new SimpleDateFormat("HH:mm:ss.SSS", Locale.US).format(new Date());
    }

    public static String clean(String s) {
        return ANSI.matcher(s).replaceAll("");
    }

    public static synchronized void write(String s) {
        if (logDir == null) {
            return;
        }
        FileOutputStream fos = null;
        try {
            File f = new File(logDir, "app.log");
            if (f.length() > 4L * 1024 * 1024) {
                File old = new File(logDir, "app.log.1");
                old.delete();
                f.renameTo(old);
            }
            fos = new FileOutputStream(f, true);
            fos.write((stamp() + "  " + clean(s) + "\n").getBytes("UTF-8"));
        } catch (Exception ignored) {
        } finally {
            try {
                if (fos != null) {
                    fos.close();
                }
            } catch (Exception ignored) {
            }
        }
    }

    /** 正常启动时打标记；正常停止时删除。残留 = 上次被硬杀。 */
    public static void setRunningFlag(String workdir, boolean on) {
        try {
            File f = new File(new File(workdir, "logs"), "running.flag");
            if (on) {
                FileOutputStream o = new FileOutputStream(f, false);
                o.write((new SimpleDateFormat("yyyy-MM-dd HH:mm:ss", Locale.US).format(new Date()) + "\n").getBytes("UTF-8"));
                o.close();
            } else {
                f.delete();
            }
        } catch (Exception ignored) {
        }
    }

    /** 上次是否非正常结束。返回 flag 里的时间字符串，没有则 null。 */
    public static String lastRunFlag(String workdir) {
        try {
            File f = new File(new File(workdir, "logs"), "running.flag");
            if (!f.exists()) {
                return null;
            }
            byte[] b = new byte[(int) f.length()];
            java.io.FileInputStream in = new java.io.FileInputStream(f);
            int n = in.read(b);
            in.close();
            return n > 0 ? new String(b, 0, n, "UTF-8").trim() : "未知时间";
        } catch (Exception e) {
            return null;
        }
    }
}
