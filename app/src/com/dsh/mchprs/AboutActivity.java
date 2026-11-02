package com.dsh.mchprs;

import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import android.content.res.ColorStateList;
import android.graphics.drawable.ColorDrawable;
import android.graphics.drawable.RippleDrawable;
import android.net.Uri;
import android.os.Bundle;
import android.util.TypedValue;
import android.view.MenuItem;
import android.view.View;
import android.view.ViewGroup;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;

/**
 * 关于界面（FEAT-017 / 018 / 019 / 020）。
 * 条目：第一行颜色字（标题），第二行灰色字（说明），第三行括号内链接，整行可点开链接。
 */
public class AboutActivity extends Activity {

    private static final int COLOR_TITLE = 0xFF39C5BB;
    private static final int COLOR_DESC = 0xFF9E9E9E;
    private static final int COLOR_LINK = 0xFF7F8C8D;
    private static final int RIPPLE = 0x26FFFFFF;

    /** 一条记录：标题 / 说明（可空） / 链接（可空）。 */
    private static final class Entry {
        final String title;
        final String desc;
        final String url;

        Entry(String title, String desc, String url) {
            this.title = title;
            this.desc = desc;
            this.url = url;
        }
    }

    private Entry[] projectEntries;
    private Entry[] thanksEntries;
    private Entry[] followEntries;

    @Override
    protected void attachBaseContext(Context base) {
        super.attachBaseContext(Lang.wrap(base));
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        if (getActionBar() != null) {
            getActionBar().setDisplayHomeAsUpEnabled(true);
            getActionBar().setTitle(R.string.about_title);
        }

        projectEntries = new Entry[]{
                new Entry("Github", null,
                        "https://github.com/xanax2112/MCHPRS_IO"),
        };
        thanksEntries = new Entry[]{
                new Entry("MCHPRS", getString(R.string.desc_upstream),
                        "http://github.com/MCHPR/MCHPRS"),
                new Entry("Geyser", getString(R.string.desc_geyser),
                        "https://github.com/GeyserMC/Geyser"),
                new Entry("bedrock-rs", getString(R.string.desc_bedrock_rs),
                        "https://github.com/bedrock-crustaceans/bedrock-rs"),
        };
        followEntries = new Entry[]{
                new Entry("Xanax21", getString(R.string.desc_developer),
                        "https://b23.tv/D3hcZ1H"),
                new Entry("Fishduoduo（YDD）", getString(R.string.desc_testing),
                        "https://b23.tv/kH9WfGh"),
        };

        LinearLayout list = new LinearLayout(this);
        list.setOrientation(LinearLayout.VERTICAL);

        section(list, getString(R.string.about_project), projectEntries);
        section(list, getString(R.string.about_thanks), thanksEntries);
        section(list, getString(R.string.about_follow), followEntries);

        ScrollView sv = new ScrollView(this);
        sv.setBackgroundColor(0xFF101418);
        sv.addView(list, new ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT));
        setContentView(sv);
    }

    private void section(LinearLayout parent, String title, Entry[] entries) {
        header(parent, title);
        for (Entry e : entries) {
            entry(parent, e);
        }
    }

    /** 区块标题：颜色字，18sp，无灰字。 */
    private void header(LinearLayout parent, String text) {
        TextView v = new TextView(this);
        v.setText(text);
        v.setTextColor(COLOR_TITLE);
        v.setTextSize(TypedValue.COMPLEX_UNIT_SP, 18);
        v.setPadding(dp(18), dp(18), dp(18), dp(6));
        parent.addView(v, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT));
    }

    /** 条目：颜色字标题 + 灰色说明 + 括号链接，整行可点。 */
    private void entry(LinearLayout parent, final Entry e) {
        LinearLayout item = new LinearLayout(this);
        item.setOrientation(LinearLayout.VERTICAL);
        item.setPadding(dp(18), dp(10), dp(18), dp(10));

        TextView t = new TextView(this);
        t.setText(e.title);
        t.setTextColor(COLOR_TITLE);
        t.setTextSize(TypedValue.COMPLEX_UNIT_SP, 16);
        item.addView(t);

        if (e.desc != null && !e.desc.isEmpty()) {
            TextView d = new TextView(this);
            d.setText(e.desc);
            d.setTextColor(COLOR_DESC);
            d.setTextSize(TypedValue.COMPLEX_UNIT_SP, 13);
            d.setPadding(0, dp(4), 0, 0);
            item.addView(d);
        }

        if (e.url != null && !e.url.isEmpty()) {
            TextView u = new TextView(this);
            u.setText("（" + e.url + "）");
            u.setTextColor(COLOR_LINK);
            u.setTextSize(TypedValue.COMPLEX_UNIT_SP, 12);
            u.setPadding(0, dp(4), 0, 0);
            item.addView(u);

            item.setClickable(true);
            item.setBackground(new RippleDrawable(
                    ColorStateList.valueOf(RIPPLE), null,
                    new ColorDrawable(0xFFFFFFFF)));
            item.setOnClickListener(new View.OnClickListener() {
                @Override
                public void onClick(View v) {
                    open(e.url);
                }
            });
        }

        parent.addView(item, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT));
    }

    private void open(String url) {
        try {
            Intent i = new Intent(Intent.ACTION_VIEW, Uri.parse(url));
            i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            startActivity(i);
        } catch (Exception ex) {
            Toast.makeText(this,
                    getString(R.string.open_link_failed, String.valueOf(ex)),
                    Toast.LENGTH_LONG).show();
        }
    }

    private int dp(int v) {
        return (int) (v * getResources().getDisplayMetrics().density);
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
