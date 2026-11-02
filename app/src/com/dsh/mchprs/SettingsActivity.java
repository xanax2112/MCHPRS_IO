package com.dsh.mchprs;

import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import android.content.res.ColorStateList;
import android.graphics.drawable.ColorDrawable;
import android.graphics.drawable.RippleDrawable;
import android.net.Uri;
import android.os.Bundle;
import android.os.Environment;
import android.provider.DocumentsContract;
import android.util.TypedValue;
import android.view.MenuItem;
import android.view.View;
import android.view.ViewGroup;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;

import java.io.File;

/**
 * 独立设置界面（FEAT-005 / 009 / 013 / 015）。
 * 行样式：整行全宽、标题在上、小一号灰字在下、波纹限制在行内。
 */
public class SettingsActivity extends Activity {

    private static final int COLOR_TITLE = 0xFF39C5BB; // 青绿
    private static final int COLOR_DESC = 0xFF9E9E9E;
    private static final int RIPPLE = 0x26FFFFFF; // 15% 白
    private static final int REQ_PICK_DIR = 21;
    private static final long RIPPLE_DELAY = 300L;

    private TextView workdirDesc;

    @Override
    protected void attachBaseContext(Context base) {
        super.attachBaseContext(Lang.wrap(base));
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        if (getActionBar() != null) {
            getActionBar().setDisplayHomeAsUpEnabled(true);
            getActionBar().setTitle(R.string.settings_title);
        }

        LinearLayout list = new LinearLayout(this);
        list.setOrientation(LinearLayout.VERTICAL);

        // ── 语言：标题固定英文 ──
        LinearLayout langItem = row(list, "Language", currentLangLabel());
        langItem.setOnClickListener(new View.OnClickListener() {
            @Override
            public void onClick(final View v) {
                Lang.toggle(SettingsActivity.this);
                // 立刻 recreate 会在波纹画完前销毁窗口，正常轻点就看不到波纹
                v.postDelayed(new Runnable() {
                    @Override
                    public void run() {
                        recreate();
                    }
                }, RIPPLE_DELAY);
            }
        });

        // ── 工作目录：点击调系统目录选择器 ──
        LinearLayout workdirItem = row(list, getString(R.string.row_workdir), workdirDescText());
        workdirDesc = (TextView) workdirItem.getChildAt(1);
        workdirItem.setOnClickListener(new View.OnClickListener() {
            @Override
            public void onClick(final View v) {
                v.postDelayed(new Runnable() {
                    @Override
                    public void run() {
                        pickDirectory();
                    }
                }, RIPPLE_DELAY);
            }
        });

        row(list, getString(R.string.row_hints), getString(R.string.desc_hints));

        // 关于：放最后，灰色说明只写版本号，点击进入独立界面
        LinearLayout aboutItem = row(list, getString(R.string.row_about),
                getString(R.string.about_desc_version, versionNameOnly()));
        aboutItem.setOnClickListener(new View.OnClickListener() {
            @Override
            public void onClick(final View v) {
                v.postDelayed(new Runnable() {
                    @Override
                    public void run() {
                        startActivity(new Intent(SettingsActivity.this, AboutActivity.class));
                    }
                }, RIPPLE_DELAY);
            }
        });

        ScrollView sv = new ScrollView(this);
        sv.setBackgroundColor(0xFF101418);
        sv.addView(list, new ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT));
        setContentView(sv);
    }

    private String workdirDescText() {
        String p = WorkDir.get(this);
        File f = new File(p);
        return f.isDirectory()
                ? getString(R.string.desc_workdir_exists, p)
                : getString(R.string.desc_workdir_missing, p);
    }

    private void pickDirectory() {
        try {
            Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
            i.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION
                    | Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
            startActivityForResult(i, REQ_PICK_DIR);
        } catch (Exception e) {
            toast(getString(R.string.pick_no_picker, String.valueOf(e)));
        }
    }

    @Override
    protected void onActivityResult(int request, int result, Intent data) {
        if (request != REQ_PICK_DIR) {
            super.onActivityResult(request, result, data);
            return;
        }
        if (result != RESULT_OK || data == null || data.getData() == null) {
            return;
        }
        Uri uri = data.getData();
        String path = uriToPath(uri);

        if (path == null) {
            toast(getString(R.string.pick_failed_parse, uri.toString()));
            return;
        }
        File dir = new File(path);
        if (!dir.isDirectory()) {
            toast(getString(R.string.pick_failed_missing, path));
            return;
        }
        if (!dir.canWrite()) {
            toast(getString(R.string.pick_failed_nowrite, path));
            return;
        }

        WorkDir.set(this, path);
        toast(getString(R.string.pick_ok, path));
        if (workdirDesc != null) {
            workdirDesc.setText(workdirDescText());
        }
    }

    /** SAF 树 URI 还原成真实路径。 */
    private String uriToPath(Uri uri) {
        if (uri == null) {
            return null;
        }
        if ("file".equals(uri.getScheme())) {
            return uri.getPath();
        }
        String docId = null;
        try {
            docId = DocumentsContract.getTreeDocumentId(uri);
        } catch (Exception ignored) {
        }
        if (docId == null) {
            String p = uri.getPath();
            if (p == null) {
                return null;
            }
            int i = p.indexOf("/tree/");
            if (i < 0) {
                return null;
            }
            docId = Uri.decode(p.substring(i + 6));
        }
        int colon = docId.indexOf(':');
        if (colon < 0) {
            return null;
        }
        String volume = docId.substring(0, colon);
        String rel = docId.substring(colon + 1);
        rel = rel.startsWith("/") ? rel.substring(1) : rel;

        if ("primary".equalsIgnoreCase(volume)) {
            String root = Environment.getExternalStorageDirectory().getAbsolutePath();
            return rel.isEmpty() ? root : root + "/" + rel;
        }
        return rel.isEmpty() ? "/storage/" + volume : "/storage/" + volume + "/" + rel;
    }

    private String currentLangLabel() {
        return Lang.ZH.equals(Lang.current(this))
                ? getString(R.string.lang_zh) : getString(R.string.lang_en);
    }

    /** 一整行：标题 + 小一号灰字说明，占满宽度，波纹限制在行内。 */
    private LinearLayout row(LinearLayout parent, String title, String desc) {
        LinearLayout item = new LinearLayout(this);
        item.setOrientation(LinearLayout.VERTICAL);
        item.setPadding(dp(18), dp(16), dp(18), dp(16));
        item.setClickable(true);
        applyRipple(item);

        TextView t = new TextView(this);
        t.setText(title);
        t.setTextColor(COLOR_TITLE);
        t.setTextSize(TypedValue.COMPLEX_UNIT_SP, 16);
        t.setClickable(false);
        t.setFocusable(false);
        item.addView(t);

        TextView d = new TextView(this);
        d.setText(desc);
        d.setTextColor(COLOR_DESC);
        d.setTextSize(TypedValue.COMPLEX_UNIT_SP, 13);
        d.setPadding(0, dp(5), 0, 0);
        // 不能 setTextIsSelectable：可选中文本会消费触摸，父容器收不到点击
        d.setClickable(false);
        d.setFocusable(false);
        d.setLongClickable(false);
        item.addView(d);

        parent.addView(item, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT));
        return item;
    }

    private void applyRipple(View v) {
        // 第三参数 mask 不可省略，否则波纹无边界，会扩散成一个大圆
        v.setBackground(new RippleDrawable(
                ColorStateList.valueOf(RIPPLE), null,
                new ColorDrawable(0xFFFFFFFF)));
    }

    private int dp(int v) {
        return (int) (v * getResources().getDisplayMetrics().density);
    }

    private void toast(String s) {
        Toast.makeText(this, s, Toast.LENGTH_LONG).show();
    }

    private String versionNameOnly() {
        try {
            return getPackageManager().getPackageInfo(getPackageName(), 0).versionName;
        } catch (Exception e) {
            return "?";
        }
    }

    private String versionInfo() {
        try {
            android.content.pm.PackageInfo pi =
                    getPackageManager().getPackageInfo(getPackageName(), 0);
            return pi.versionName + " (" + pi.versionCode + ")";
        } catch (Exception e) {
            return "?";
        }
    }

    @Override
    public boolean onOptionsItemSelected(MenuItem item) {
        if (item.getItemId() == android.R.id.home) {
            finish();
            return true;
        }
        return super.onOptionsItemSelected(item);
    }
}
