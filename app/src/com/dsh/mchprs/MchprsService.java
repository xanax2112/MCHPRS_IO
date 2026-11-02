package com.dsh.mchprs;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.os.Build;
import android.os.IBinder;
import android.os.PowerManager;

import java.io.BufferedReader;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.io.Writer;
import java.util.LinkedList;

/**
 * 前台服务：托管 mchprs 子进程。
 * v0.7：界面只显示服务器自身输出；App 诊断只进日志文件；启动结果用状态回传。
 */
public class MchprsService extends Service {

    public static final String ACTION_START = "com.dsh.mchprs.START";
    public static final String ACTION_STOP = "com.dsh.mchprs.STOP";
    public static final String EXTRA_WORKDIR = "workdir";

    public static final int ST_IDLE = 0;
    public static final int ST_STARTING = 1;
    public static final int ST_READY = 2;
    public static final int ST_FAILED = 3;
    public static final int ST_STOPPING = 4;

    private static final String CH_ID = "mchprs";
    private static final int NOTI_ID = 1001;
    private static final int MAX_LINES = 2000;
    private static final int SIGINT = 2;
    private static final int SIGTERM = 15;

    private static final LinkedList<String> SERVER_LINES = new LinkedList<String>();
    private static MchprsService instance;
    private static Listener listener;

    public interface Listener {
        void onLine(String line);

        void onStatus(int state, String detail);
    }

    private Process proc;
    /** 子进程 stdin，用来把界面输入的命令送进服务器的终端控制台。 */
    private Writer cmdWriter;
    private PowerManager.WakeLock wake;
    private String workdir = MainActivity.DEFAULT_WORKDIR;
    private volatile long startedAt = 0L;
    private volatile boolean heartbeatOn = false;
    private volatile int state = ST_IDLE;
    private volatile String stateDetail = "";

    public static void setListener(Listener l) {
        listener = l;
    }

    public static void clearListener(Listener l) {
        if (listener == l) {
            listener = null;
        }
    }

    public static boolean isRunning() {
        return instance != null && instance.proc != null;
    }

    /**
     * 把一行文本写进子进程 stdin。服务器只执行以 / 开头的行，其余静默忽略 ——
     * 这里原样发送，不补斜杠也不做任何改写。
     *
     * @return false 表示进程未运行或写入失败
     */
    public static boolean sendCommand(String line) {
        MchprsService s = instance;
        if (s == null || s.proc == null || s.cmdWriter == null || line == null) {
            return false;
        }
        try {
            // 换行是服务器按行读取的边界，写完必须 flush
            s.cmdWriter.write(line);
            s.cmdWriter.write("\n");
            s.cmdWriter.flush();
            return true;
        } catch (Exception e) {
            s.appLog("写入 stdin 失败: " + e);
            return false;
        }
    }

    /** 只在界面上回显一行（本地输入），不写日志文件。 */
    public static void echoLine(String text) {
        String stamped = Journal.stamp() + "  " + text;
        synchronized (SERVER_LINES) {
            SERVER_LINES.add(stamped);
            while (SERVER_LINES.size() > MAX_LINES) {
                SERVER_LINES.removeFirst();
            }
        }
        Listener l = listener;
        if (l != null) {
            l.onLine(stamped);
        }
    }

    public static int getState() {
        return instance == null ? ST_IDLE : instance.state;
    }

    public static String getDetail() {
        return instance == null ? "" : instance.stateDetail;
    }

    public static String[] serverSnapshot() {
        synchronized (SERVER_LINES) {
            return SERVER_LINES.toArray(new String[0]);
        }
    }

    public static void clearServerLines() {
        synchronized (SERVER_LINES) {
            SERVER_LINES.clear();
        }
    }

    @Override
    public IBinder onBind(Intent intent) {
        return null;
    }

    @Override
    public void onCreate() {
        super.onCreate();
        instance = this;
        Journal.init(workdir);
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        String action = intent == null ? "null(系统重启)" : String.valueOf(intent.getAction());
        appLog("onStartCommand action=" + action + " startId=" + startId);

        if (intent != null && ACTION_STOP.equals(intent.getAction())) {
            appLog("收到停止请求");
            Journal.setRunningFlag(workdir, false);
            shutdownGracefully(true);
            return START_NOT_STICKY;
        }

        if (intent != null && intent.getStringExtra(EXTRA_WORKDIR) != null) {
            workdir = intent.getStringExtra(EXTRA_WORKDIR);
        } else {
            workdir = WorkDir.get(this);
        }
        Journal.init(workdir);

        try {
            startForeground(NOTI_ID, buildNotification(Lang.str(MchprsService.this, R.string.noti_starting)));
        } catch (Exception e) {
            appLog("startForeground 失败: " + e);
        }

        String last = Journal.lastRunFlag(workdir);
        if (last != null) {
            appLog("上次运行未正常结束（标记 " + last + "）");
        }
        ensureStarted();
        return START_STICKY;
    }

    private void ensureStarted() {
        if (proc != null) {
            updateNotification(Lang.str(MchprsService.this, R.string.noti_running));
            return;
        }

        setState(ST_STARTING, "");

        File dir = new File(workdir);
        File bin = new File(getApplicationInfo().nativeLibraryDir + "/libmchprs.so");
        appLog("bin=" + bin.getAbsolutePath() + " exists=" + bin.exists()
                + " exec=" + bin.canExecute());
        appLog("workdir=" + workdir + " isDir=" + dir.isDirectory());

        if (!dir.isDirectory()) {
            appLog("工作目录不存在，尝试创建: " + workdir);
            boolean created;
            try {
                created = dir.mkdirs();
            } catch (SecurityException se) {
                created = false;
                appLog("创建失败(无权限): " + se);
            }
            if (!created) {
                if (dir.isDirectory()) {
                    appLog("目录已由他处创建");
                } else {
                    fail(Lang.str(MchprsService.this, R.string.err_workdir_create));
                    return;
                }
            } else {
                appLog("工作目录已创建");
            }
        }
        if (!bin.exists()) {
            fail(Lang.str(MchprsService.this, R.string.err_bin_missing));
            return;
        }

        try {
            ProcessBuilder pb = new ProcessBuilder(bin.getAbsolutePath());
            pb.directory(dir);
            pb.redirectErrorStream(true);
            proc = pb.start();
        } catch (Exception e) {
            proc = null;
            fail(Lang.str(MchprsService.this, R.string.err_process_create, String.valueOf(e)));
            return;
        }

        startedAt = System.currentTimeMillis();
        try {
            // stdin 保持为管道且永不 close：一旦 EOF，服务器的 console 线程会退出
            cmdWriter = new OutputStreamWriter(proc.getOutputStream(), "UTF-8");
        } catch (Exception e) {
            cmdWriter = null;
            appLog("获取 stdin 失败: " + e);
        }
        appLog("进程已创建 " + childInfo());
        Journal.setRunningFlag(workdir, true);
        acquireWakeLock();
        pump(proc);
        startHeartbeat();
        updateNotification(Lang.str(MchprsService.this, R.string.noti_starting));
    }

    private void fail(String reason) {
        appLog("启动失败: " + reason);
        setState(ST_FAILED, reason);
        updateNotification(Lang.str(MchprsService.this, R.string.noti_failed));
        releaseWakeLock();
    }

    private void setState(int st, String detail) {
        state = st;
        stateDetail = detail == null ? "" : detail;
        Listener l = listener;
        if (l != null) {
            l.onStatus(st, stateDetail);
        }
    }

    private void shutdownGracefully(final boolean stopServiceAfter) {
        heartbeatOn = false;
        updateNotification(Lang.str(MchprsService.this, R.string.noti_stopping));
        setState(ST_STOPPING, "");
        final Process p = proc;
        final int pid = findChildPid();

        if (p == null) {
            setState(ST_IDLE, "");
            releaseWakeLock();
            if (stopServiceAfter) {
                stopSelf();
            }
            return;
        }

        appLog("发送 SIGINT 给 pid=" + pid + "，等待存档");
        boolean sent = false;
        if (pid > 0) {
            try {
                android.os.Process.sendSignal(pid, SIGINT);
                sent = true;
            } catch (Exception e) {
                appLog("SIGINT 失败: " + e);
            }
        }
        if (!sent) {
            try {
                p.destroy();
            } catch (Exception ignored) {
            }
        }

        new Thread(new Runnable() {
            @Override
            public void run() {
                boolean exited = waitExit(p, 20000);
                if (!exited) {
                    appLog("20 秒未退出，发 SIGTERM");
                    if (pid > 0) {
                        try {
                            android.os.Process.sendSignal(pid, SIGTERM);
                        } catch (Exception ignored) {
                        }
                    }
                    exited = waitExit(p, 8000);
                }
                if (!exited) {
                    appLog("仍未退出，SIGKILL");
                    try {
                        p.destroyForcibly();
                    } catch (Exception ignored) {
                    }
                } else {
                    appLog("进程已正常退出");
                }
                releaseWakeLock();
                setState(ST_IDLE, "");
                if (stopServiceAfter) {
                    stopSelf();
                }
            }
        }, "shutdown").start();
    }

    private boolean waitExit(Process p, long timeoutMs) {
        long end = System.currentTimeMillis() + timeoutMs;
        while (System.currentTimeMillis() < end) {
            try {
                p.exitValue();
                return true;
            } catch (IllegalThreadStateException alive) {
                // 还在跑
            } catch (Throwable t) {
                return true;
            }
            try {
                Thread.sleep(200L);
            } catch (InterruptedException e) {
                return false;
            }
        }
        try {
            p.exitValue();
            return true;
        } catch (Exception e) {
            return false;
        }
    }

    private int findChildPid() {
        File procDir = new File("/proc");
        String[] names = procDir.list();
        if (names == null) {
            return -1;
        }
        int myUid = android.os.Process.myUid();
        for (String n : names) {
            if (n.isEmpty() || !Character.isDigit(n.charAt(0))) {
                continue;
            }
            try {
                String cmd = readText(new File("/proc/" + n + "/cmdline"));
                if (cmd == null || !cmd.contains("libmchprs.so")) {
                    continue;
                }
                String status = readText(new File("/proc/" + n + "/status"));
                if (status == null) {
                    continue;
                }
                for (String line : status.split("\n")) {
                    if (line.startsWith("Uid:")) {
                        String[] parts = line.trim().split("\\s+");
                        if (parts.length > 1 && Integer.parseInt(parts[1]) == myUid) {
                            return Integer.parseInt(n);
                        }
                    }
                }
            } catch (Exception ignored) {
            }
        }
        return -1;
    }

    private String childInfo() {
        int pid = findChildPid();
        if (pid < 0) {
            return "pid=未找到";
        }
        try {
            String s = readText(new File("/proc/" + pid + "/stat"));
            if (s == null) {
                return "pid=" + pid;
            }
            int i = s.lastIndexOf(')');
            String[] f = s.substring(i + 2).trim().split("\\s+");
            long ut = Long.parseLong(f[11]);
            long st = Long.parseLong(f[12]);
            return "pid=" + pid + " state=" + f[0] + " cpuTicks=" + (ut + st);
        } catch (Exception e) {
            return "pid=" + pid;
        }
    }

    private static String readText(File f) {
        FileInputStream in = null;
        try {
            in = new FileInputStream(f);
            ByteArrayOutputStream bos = new ByteArrayOutputStream();
            byte[] buf = new byte[4096];
            int n;
            while ((n = in.read(buf)) > 0) {
                bos.write(buf, 0, n);
            }
            return new String(bos.toByteArray(), "UTF-8").replace('\u0000', ' ');
        } catch (Exception e) {
            return null;
        } finally {
            try {
                if (in != null) {
                    in.close();
                }
            } catch (Exception ignored) {
            }
        }
    }

    private void startHeartbeat() {
        if (heartbeatOn) {
            return;
        }
        heartbeatOn = true;
        new Thread(new Runnable() {
            @Override
            public void run() {
                while (heartbeatOn) {
                    try {
                        Thread.sleep(30000L);
                    } catch (InterruptedException e) {
                        break;
                    }
                    if (!heartbeatOn) {
                        break;
                    }
                    long up = startedAt == 0 ? 0 : (System.currentTimeMillis() - startedAt) / 1000;
                    appLog("心跳 uptime=" + up + "s " + childInfo()
                            + " wake=" + (wake != null && wake.isHeld()));
                }
            }
        }, "heartbeat").start();
    }

    private void pump(final Process p) {
        new Thread(new Runnable() {
            @Override
            public void run() {
                try {
                    BufferedReader r = new BufferedReader(new InputStreamReader(p.getInputStream()));
                    String line;
                    while ((line = r.readLine()) != null) {
                        serverLog(line);
                        if (line.contains("Done! Start took")) {
                            setState(ST_READY, "");
                            updateNotification(Lang.str(MchprsService.this, R.string.noti_running));
                        }
                    }
                } catch (Exception e) {
                    appLog("读取输出结束: " + e);
                }
                appLog("子进程已退出");
                if (proc == p) {
                    proc = null;
                    cmdWriter = null;
                }
                heartbeatOn = false;
                releaseWakeLock();
                if (state != ST_STOPPING) {
                    setState(ST_FAILED, "服务器进程已退出");
                    updateNotification(Lang.str(MchprsService.this, R.string.noti_stopped));
                } else {
                    setState(ST_IDLE, "");
                    updateNotification(Lang.str(MchprsService.this, R.string.noti_stopped));
                }
            }
        }, "mchprs-reader").start();
    }

    private void acquireWakeLock() {
        try {
            PowerManager pm = (PowerManager) getSystemService(Context.POWER_SERVICE);
            wake = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "mchprs:server");
            wake.acquire();
        } catch (Exception e) {
            appLog("WakeLock 失败: " + e);
        }
    }

    private void releaseWakeLock() {
        try {
            if (wake != null && wake.isHeld()) {
                wake.release();
            }
        } catch (Exception ignored) {
        }
        wake = null;
    }

    /** App 自己的诊断，只进日志文件，不上界面。 */
    private void appLog(String s) {
        Journal.write("[app] " + s);
    }

    /** 服务器输出，进日志 + 界面缓冲。 */
    private void serverLog(String s) {
        String clean = Journal.clean(s);
        String stamped = Journal.stamp() + "  " + clean;
        synchronized (SERVER_LINES) {
            SERVER_LINES.add(stamped);
            while (SERVER_LINES.size() > MAX_LINES) {
                SERVER_LINES.removeFirst();
            }
        }
        Journal.write(clean);
        Listener l = listener;
        if (l != null) {
            l.onLine(stamped);
        }
    }

    private Notification buildNotification(String text) {
        Intent open = new Intent(this, MainActivity.class);
        PendingIntent pi = PendingIntent.getActivity(this, 0, open,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);

        Intent stop = new Intent(this, MchprsService.class);
        stop.setAction(ACTION_STOP);
        PendingIntent psi = PendingIntent.getService(this, 1, stop,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);

        Notification.Builder b;
        if (Build.VERSION.SDK_INT >= 26) {
            NotificationManager nm = (NotificationManager) getSystemService(Context.NOTIFICATION_SERVICE);
            NotificationChannel ch = new NotificationChannel(CH_ID, Lang.str(MchprsService.this, R.string.noti_channel),
                    NotificationManager.IMPORTANCE_LOW);
            ch.setShowBadge(false);
            nm.createNotificationChannel(ch);
            b = new Notification.Builder(this, CH_ID);
        } else {
            b = new Notification.Builder(this);
        }
        b.setContentTitle("MCHPRS_IO")
                .setContentText(text)
                .setSmallIcon(android.R.drawable.stat_notify_sync)
                .setOngoing(true)
                .setContentIntent(pi)
                .addAction(android.R.drawable.ic_menu_close_clear_cancel, Lang.str(MchprsService.this, R.string.noti_stop_action), psi);
        return b.build();
    }

    private void updateNotification(String text) {
        try {
            NotificationManager nm = (NotificationManager) getSystemService(Context.NOTIFICATION_SERVICE);
            nm.notify(NOTI_ID, buildNotification(text));
        } catch (Exception ignored) {
        }
    }

    @Override
    public void onTaskRemoved(Intent rootIntent) {
        appLog("onTaskRemoved");
        super.onTaskRemoved(rootIntent);
    }

    @Override
    public void onDestroy() {
        appLog("onDestroy 服务销毁");
        if (proc != null) {
            int pid = findChildPid();
            if (pid > 0) {
                try {
                    android.os.Process.sendSignal(pid, SIGINT);
                } catch (Exception ignored) {
                }
            }
            if (!waitExit(proc, 5000)) {
                try {
                    proc.destroyForcibly();
                } catch (Exception ignored) {
                }
            }
        }
        heartbeatOn = false;
        releaseWakeLock();
        instance = null;
        super.onDestroy();
    }
}
