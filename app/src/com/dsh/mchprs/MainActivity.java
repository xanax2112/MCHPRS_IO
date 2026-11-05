package com.dsh.mchprs;

import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.content.res.ColorStateList;
import android.graphics.Typeface;
import android.graphics.drawable.Drawable;
import android.graphics.drawable.GradientDrawable;
import android.graphics.drawable.RippleDrawable;
import android.os.Build;
import android.os.Bundle;
import android.text.Editable;
import android.text.InputFilter;
import android.text.InputType;
import android.text.Spanned;
import android.text.TextWatcher;
import android.util.TypedValue;
import android.view.KeyEvent;
import android.view.Menu;
import android.view.MenuItem;
import android.view.View;
import android.view.ViewGroup;
import android.view.inputmethod.EditorInfo;
import android.widget.Button;
import android.widget.EditText;
import android.widget.HorizontalScrollView;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;

import java.io.File;

public class MainActivity extends Activity implements MchprsService.Listener {

    public static final String DEFAULT_WORKDIR = "/storage/emulated/0/termux/MCHPRS_IO";
    private static final int REQ_STORAGE = 1;
    private static final int MENU_SETTINGS = 100;

    private static final int COLOR_BLUE = 0xFF8AB4F8;
    private static final int COLOR_GREEN = 0xFF66BB6A;
    private static final int COLOR_RED = 0xFFEF5350;
    private static final int COLOR_GREY = 0xFF9E9E9E;
    /** 与标题一致的青蓝色。 */
    private static final int COLOR_CYAN = 0xFF39C5BB;

    /** 快捷指令：显示文本（用于前缀匹配）。 */
    private static final String[] QUICK_CMDS = {
            "/redpiler compile", "/redpiler reset", "/rtps", "/tick seq",
            "//update -p"
    };
    /** 快捷指令：点击后真正填进输入框的文本（/rtps 带一个尾随空格，光标直接停到参数位）。 */
    private static final String[] QUICK_FILL = {
            "/redpiler compile", "/redpiler reset", "/rtps ", "/tick seq",
            "//update -p"
    };

    private TextView status;
    private TextView console;
    private ScrollView scroller;
    private Button toggle;
    private EditText cmdInput;
    private LinearLayout cmdHints;
    private String workdir = DEFAULT_WORKDIR;
    private String appliedLang;

    @Override
    protected void attachBaseContext(Context base) {
        super.attachBaseContext(Lang.wrap(base));
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        appliedLang = Lang.current(this);
        workdir = WorkDir.get(this);

        Journal.init(workdir);
        Journal.write("[app] lang: sys=" + Lang.systemLang(this)
                + " current=" + Lang.current(this)
                + " hasChoice=" + Lang.hasChoice(this));

        maybeExtractAssets();

        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(0xFF101418);
        root.setPadding(dp(14), dp(14), dp(14), dp(14));

        status = new TextView(this);
        status.setTextSize(TypedValue.COMPLEX_UNIT_SP, 15);
        status.setTypeface(Typeface.DEFAULT_BOLD);
        status.setPadding(0, 0, 0, dp(10));

        toggle = new Button(this);
        toggle.setOnClickListener(new View.OnClickListener() {
            @Override
            public void onClick(View v) {
                onToggle();
            }
        });

        Button clear = new Button(this);
        clear.setText(R.string.btn_clear);
        clear.setOnClickListener(new View.OnClickListener() {
            @Override
            public void onClick(View v) {
                MchprsService.clearServerLines();
                console.setText("");
            }
        });

        console = new TextView(this);
        console.setTextColor(0xFFD0D0D0);
        console.setTextSize(TypedValue.COMPLEX_UNIT_SP, 13);
        console.setTypeface(Typeface.MONOSPACE);
        console.setTextIsSelectable(true);

        scroller = new ScrollView(this);
        scroller.addView(console, new ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT));

        // ── 底部命令输入行 ──
        cmdInput = new EditText(this);
        cmdInput.setHint(R.string.cmd_hint);
        cmdInput.setHintTextColor(0xFF6B7278);
        cmdInput.setTextColor(0xFFE6E6E6);
        cmdInput.setTextSize(TypedValue.COMPLEX_UNIT_SP, 14);
        cmdInput.setTypeface(Typeface.MONOSPACE);
        cmdInput.setBackground(roundRect(0xFF1C2228, 8, 0xFF2A3138));
        cmdInput.setPadding(dp(10), dp(8), dp(10), dp(8));
        // 多行模式：软键盘不会把回车变成「发送」动作，按键本身也被下面的监听吃掉
        cmdInput.setSingleLine(false);
        cmdInput.setImeOptions(EditorInfo.IME_ACTION_NONE);
        cmdInput.setInputType(InputType.TYPE_CLASS_TEXT
                | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS);
        cmdInput.setFilters(new InputFilter[]{new InputFilter() {
            @Override
            public CharSequence filter(CharSequence src, int start, int end,
                                       Spanned dst, int dstart, int dend) {
                String s = src.subSequence(start, end).toString()
                        .replace("\n", "").replace("\r", "");
                return s.length() == end - start ? null : s;
            }
        }});
        cmdInput.setOnKeyListener(new View.OnKeyListener() {
            @Override
            public boolean onKey(View v, int keyCode, KeyEvent event) {
                // 回车（含小键盘回车）一律吞掉，不换行也不发送
                return keyCode == KeyEvent.KEYCODE_ENTER
                        || keyCode == KeyEvent.KEYCODE_NUMPAD_ENTER;
            }
        });

        // 青蓝色小按钮：只负责发送，靠它而不是回车
        Button send = new Button(this);
        send.setText("\u25B6");
        send.setTextColor(0xFF101418);
        send.setTextSize(TypedValue.COMPLEX_UNIT_SP, 13);
        send.setTypeface(Typeface.DEFAULT_BOLD);
        send.setPadding(0, 0, 0, 0);
        send.setMinWidth(0);
        send.setMinimumWidth(0);
        send.setMinHeight(0);
        send.setMinimumHeight(0);
        if (Build.VERSION.SDK_INT >= 21) {
            send.setStateListAnimator(null);
        }
        Drawable sendBg = roundRect(COLOR_CYAN, 8, COLOR_CYAN);
        send.setBackground(new RippleDrawable(
                ColorStateList.valueOf(0x33FFFFFF), sendBg, sendBg));
        send.setOnClickListener(new View.OnClickListener() {
            @Override
            public void onClick(View v) {
                submitCommand();
            }
        });

        // 快捷指令建议栏：输入 / 开头时出现，点一下只填充、不发送
        cmdHints = new LinearLayout(this);
        cmdHints.setOrientation(LinearLayout.HORIZONTAL);
        cmdHints.setPadding(0, dp(8), 0, 0);
        cmdHints.setVisibility(View.GONE);
        for (int i = 0; i < QUICK_CMDS.length; i++) {
            cmdHints.addView(makeChip(i));
        }
        HorizontalScrollView hintScroll = new HorizontalScrollView(this);
        hintScroll.setHorizontalScrollBarEnabled(false);
        hintScroll.setVisibility(View.GONE);
        hintScroll.addView(cmdHints);

        cmdInput.addTextChangedListener(new TextWatcher() {
            @Override
            public void beforeTextChanged(CharSequence s, int a, int b, int c) {
            }

            @Override
            public void onTextChanged(CharSequence s, int a, int b, int c) {
            }

            @Override
            public void afterTextChanged(Editable s) {
                updateHints(s.toString(), hintScroll);
            }
        });

        LinearLayout cmdRow = new LinearLayout(this);
        cmdRow.setOrientation(LinearLayout.HORIZONTAL);
        cmdRow.setPadding(0, dp(8), 0, 0);
        cmdRow.addView(cmdInput, new LinearLayout.LayoutParams(
                0, dp(40), 1f));
        LinearLayout.LayoutParams sendLp = new LinearLayout.LayoutParams(dp(44), dp(40));
        sendLp.leftMargin = dp(8);
        cmdRow.addView(send, sendLp);

        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        LinearLayout.LayoutParams half = new LinearLayout.LayoutParams(
                0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f);
        row.addView(toggle, half);
        row.addView(clear, half);

        LinearLayout.LayoutParams full = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        root.addView(status, full);
        root.addView(row, full);
        root.addView(scroller, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));
        root.addView(hintScroll, full);
        root.addView(cmdRow, full);
        setContentView(root);

        String last = Journal.lastRunFlag(workdir);
        if (last != null) {
            Journal.write("[app] 上次运行未正常结束: " + last);
        }
    }

    private int dp(int v) {
        return (int) (v * getResources().getDisplayMetrics().density);
    }

    /** 圆角矩形背景；stroke 与 fill 相同表示不描边。 */
    private Drawable roundRect(int fill, int radiusDp, int stroke) {
        GradientDrawable g = new GradientDrawable();
        g.setShape(GradientDrawable.RECTANGLE);
        g.setColor(fill);
        g.setCornerRadius(dp(radiusDp));
        if (stroke != fill) {
            g.setStroke(dp(1), stroke);
        }
        return g;
    }

    /** 快捷指令小标签：点击只填充文本，不发送。 */
    private View makeChip(final int index) {
        TextView chip = new TextView(this);
        chip.setText(QUICK_CMDS[index]);
        chip.setTextColor(COLOR_CYAN);
        chip.setTextSize(TypedValue.COMPLEX_UNIT_SP, 12);
        chip.setTypeface(Typeface.MONOSPACE);
        chip.setPadding(dp(10), dp(6), dp(10), dp(6));
        Drawable bg = roundRect(0xFF1C2228, 8, 0xFF2A3138);
        chip.setBackground(new RippleDrawable(ColorStateList.valueOf(0x33FFFFFF), bg, bg));
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        lp.rightMargin = dp(8);
        chip.setLayoutParams(lp);
        chip.setOnClickListener(new View.OnClickListener() {
            @Override
            public void onClick(View v) {
                String fill = QUICK_FILL[index];
                cmdInput.setText(fill);
                cmdInput.setSelection(fill.length());
                cmdInput.requestFocus();
            }
        });
        return chip;
    }

    /**
     * 输入 / 开头时按前缀显示建议；文本正好等于某条指令时不再显示（视为已选）。
     */
    private void updateHints(String text, View bar) {
        if (!text.startsWith("/")) {
            bar.setVisibility(View.GONE);
            return;
        }
        int shown = 0;
        for (int i = 0; i < QUICK_CMDS.length; i++) {
            boolean match = QUICK_CMDS[i].startsWith(text) && !QUICK_CMDS[i].equals(text);
            cmdHints.getChildAt(i).setVisibility(match ? View.VISIBLE : View.GONE);
            if (match) {
                shown++;
            }
        }
        bar.setVisibility(shown > 0 ? View.VISIBLE : View.GONE);
        // 内层容器默认 GONE，露出时必须一起打开，否则外层可见但里面是空的
        cmdHints.setVisibility(shown > 0 ? View.VISIBLE : View.GONE);
    }

    /** 把输入框内容原样送进服务器 stdin（不补斜杠、不改写）。 */
    private void submitCommand() {
        String text = cmdInput.getText().toString().trim();
        if (text.length() == 0) {
            return;
        }
        if (!MchprsService.isRunning()) {
            Toast.makeText(this, R.string.cmd_not_running, Toast.LENGTH_SHORT).show();
            return;
        }
        if (MchprsService.sendCommand(text)) {
            MchprsService.echoLine("> " + text);
            cmdInput.setText("");
        } else {
            Toast.makeText(this, R.string.cmd_send_failed, Toast.LENGTH_SHORT).show();
        }
    }

    /** 右上角纯文字 Settings 入口：无图标、无边框、不翻译。 */
    @Override
    public boolean onCreateOptionsMenu(Menu menu) {
        MenuItem item = menu.add(Menu.NONE, MENU_SETTINGS, Menu.NONE, "Settings");
        item.setShowAsAction(MenuItem.SHOW_AS_ACTION_ALWAYS);

        // 自定义 view 渲染，绕开 Material 主题的 textAllCaps（否则显示成 SETTINGS）
        TextView action = new TextView(this);
        action.setText("Settings");
        action.setTextColor(0xFFFFFFFF);
        action.setTextSize(TypedValue.COMPLEX_UNIT_SP, 15);
        action.setPadding(dp(18), dp(8), dp(18), dp(8));
        action.setOnClickListener(new View.OnClickListener() {
            @Override
            public void onClick(View v) {
                openSettings();
            }
        });
        item.setActionView(action);
        return true;
    }

    @Override
    public boolean onOptionsItemSelected(MenuItem item) {
        if (item.getItemId() == MENU_SETTINGS) {
            openSettings();
            return true;
        }
        return super.onOptionsItemSelected(item);
    }

    private void openSettings() {
        startActivity(new Intent(this, SettingsActivity.class));
    }

    @Override
    protected void onResume() {
        super.onResume();
        if (appliedLang != null && !appliedLang.equals(Lang.current(this))) {
            recreate();
            return;
        }
        MchprsService.setListener(this);
        Journal.write("[app] 界面 onResume");
        dumpServer();
        applyStatus(MchprsService.getState(), MchprsService.getDetail());
    }

    @Override
    protected void onPause() {
        Journal.write("[app] 界面 onPause");
        MchprsService.clearListener(this);
        super.onPause();
    }

    @Override
    protected void onDestroy() {
        Journal.write("[app] 界面 onDestroy");
        super.onDestroy();
    }

    private void dumpServer() {
        console.setText("");
        for (String s : MchprsService.serverSnapshot()) {
            console.append(s + "\n");
        }
        scrollToBottom();
    }

    private void applyStatus(final int state, final String detail) {
        runOnUiThread(new Runnable() {
            @Override
            public void run() {
                switch (state) {
                    case MchprsService.ST_STARTING:
                        status.setTextColor(COLOR_BLUE);
                        status.setText("● " + getString(R.string.state_starting));
                        toggle.setText(R.string.btn_cancel);
                        toggle.setEnabled(true);
                        break;
                    case MchprsService.ST_READY:
                        status.setTextColor(COLOR_GREEN);
                        status.setText("● " + getString(R.string.state_ready));
                        toggle.setText(R.string.btn_stop);
                        toggle.setEnabled(true);
                        break;
                    case MchprsService.ST_FAILED:
                        status.setTextColor(COLOR_RED);
                        status.setText("● " + getString(R.string.state_failed, detail));
                        toggle.setText(R.string.btn_restart);
                        toggle.setEnabled(true);
                        break;
                    case MchprsService.ST_STOPPING:
                        status.setTextColor(COLOR_BLUE);
                        status.setText("● " + getString(R.string.state_stopping));
                        toggle.setText(R.string.btn_stopping);
                        toggle.setEnabled(false);
                        break;
                    default:
                        status.setTextColor(COLOR_GREY);
                        status.setText("● " + getString(R.string.state_idle));
                        toggle.setText(R.string.btn_start);
                        toggle.setEnabled(true);
                        break;
                }
            }
        });
    }

    private void onToggle() {
        int st = MchprsService.getState();
        if (st == MchprsService.ST_READY || st == MchprsService.ST_STARTING) {
            stopServer();
        } else {
            startServer();
        }
    }

    private void startServer() {
        if (checkSelfPermission(android.Manifest.permission.WRITE_EXTERNAL_STORAGE)
                != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(new String[]{
                    android.Manifest.permission.WRITE_EXTERNAL_STORAGE,
                    android.Manifest.permission.READ_EXTERNAL_STORAGE}, REQ_STORAGE);
            return;
        }
        workdir = WorkDir.get(this);
        File dir = new File(workdir);
        if (!dir.isDirectory()) {
            Journal.write("[app] 工作目录不存在，尝试创建: " + workdir);
            if (!dir.mkdirs() && !dir.isDirectory()) {
                applyStatus(MchprsService.ST_FAILED, getString(R.string.err_workdir_create));
                return;
            }
        }
        Intent i = new Intent(this, MchprsService.class);
        i.setAction(MchprsService.ACTION_START);
        i.putExtra(MchprsService.EXTRA_WORKDIR, workdir);
        try {
            if (Build.VERSION.SDK_INT >= 26) {
                startForegroundService(i);
            } else {
                startService(i);
            }
            applyStatus(MchprsService.ST_STARTING, "");
        } catch (Exception e) {
            applyStatus(MchprsService.ST_FAILED, getString(R.string.err_service_start, String.valueOf(e)));
        }
    }

    private void stopServer() {
        Journal.write("[app] 用户点击停止");
        Intent i = new Intent(this, MchprsService.class);
        i.setAction(MchprsService.ACTION_STOP);
        try {
            startService(i);
            applyStatus(MchprsService.ST_STOPPING, "");
        } catch (Exception e) {
            applyStatus(MchprsService.ST_FAILED, getString(R.string.err_stop_request, String.valueOf(e)));
        }
    }

    /**
     * 把 APK 内打包的 docs 与 python 释放到工作目录（用户 2026-11-02 需求）。
     * 没有存储权限时跳过，等 {@link #onRequestPermissionsResult} 授权后再调用。
     */
    private void maybeExtractAssets() {
        final String root = WorkDir.get(this);
        if (checkSelfPermission(android.Manifest.permission.WRITE_EXTERNAL_STORAGE)
                != PackageManager.PERMISSION_GRANTED) {
            return;
        }
        new Thread(new Runnable() {
            @Override
            public void run() {
                int n = Assets.extract(MainActivity.this, root);
                if (n > 0) {
                    Journal.write("[app] 释放内置 docs/python " + n + " 个文件到 " + root);
                }
            }
        }, "mchprs-assets").start();
    }

    @Override
    public void onRequestPermissionsResult(int code, String[] perms, int[] res) {
        if (code == REQ_STORAGE) {
            if (res.length > 0 && res[0] == PackageManager.PERMISSION_GRANTED) {
                maybeExtractAssets();
                startServer();
            } else {
                applyStatus(MchprsService.ST_FAILED, getString(R.string.err_storage_denied));
            }
        }
    }

    @Override
    public void onLine(final String line) {
        runOnUiThread(new Runnable() {
            @Override
            public void run() {
                console.append(line + "\n");
                scrollToBottom();
            }
        });
    }

    @Override
    public void onStatus(int state, String detail) {
        applyStatus(state, detail);
        if (state == MchprsService.ST_FAILED) {
            final String msg = getString(R.string.toast_failed, detail);
            runOnUiThread(new Runnable() {
                @Override
                public void run() {
                    Toast.makeText(MainActivity.this, msg, Toast.LENGTH_LONG).show();
                }
            });
        }
    }

    private void scrollToBottom() {
        scroller.post(new Runnable() {
            @Override
            public void run() {
                scroller.fullScroll(View.FOCUS_DOWN);
            }
        });
    }
}
