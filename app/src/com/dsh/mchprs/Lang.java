package com.dsh.mchprs;

import android.content.Context;
import android.content.SharedPreferences;
import android.content.res.Configuration;
import android.content.res.Resources;

import java.util.Locale;

/**
 * 语言控制（FEAT-009 / FEAT-010）。
 * 二态：English ⇄ 简体中文。
 * 未手动选择过时按系统语言决定：系统是中文（简/繁皆可）→ 简中，其它 → 英文。
 * 不使用 AndroidX。
 */
public final class Lang {

    private static final String PREF = "settings";
    private static final String KEY_LANG = "lang";
    public static final String EN = "en";
    public static final String ZH = "zh";

    private Lang() {
    }

    /** 系统语言：zh（含 zh-TW / zh-HK / zh-Hant）算中文，其它一律英文。 */
    public static String systemLang(Context c) {
        try {
            Locale sys = Resources.getSystem().getConfiguration().getLocales().get(0);
            return "zh".equalsIgnoreCase(sys.getLanguage()) ? ZH : EN;
        } catch (Throwable t) {
            return EN;
        }
    }

    public static boolean hasChoice(Context c) {
        try {
            return c.getSharedPreferences(PREF, Context.MODE_PRIVATE).contains(KEY_LANG);
        } catch (Exception e) {
            return false;
        }
    }

    public static String current(Context c) {
        try {
            SharedPreferences sp = c.getSharedPreferences(PREF, Context.MODE_PRIVATE);
            if (!sp.contains(KEY_LANG)) {
                // 未选择过：跟随系统（只用于决定初始值）
                return systemLang(c);
            }
            String v = sp.getString(KEY_LANG, EN);
            return ZH.equals(v) ? ZH : EN;
        } catch (Exception e) {
            return EN;
        }
    }

    public static void set(Context c, String lang) {
        c.getSharedPreferences(PREF, Context.MODE_PRIVATE)
                .edit().putString(KEY_LANG, ZH.equals(lang) ? ZH : EN).commit();
    }

    public static String toggle(Context c) {
        String next = ZH.equals(current(c)) ? EN : ZH;
        set(c, next);
        return next;
    }

    public static Locale locale(Context c) {
        return ZH.equals(current(c)) ? Locale.SIMPLIFIED_CHINESE : Locale.ENGLISH;
    }

    /** 给 Activity 的 attachBaseContext 用。不修改进程级默认 Locale。 */
    public static Context wrap(Context base) {
        Configuration cfg = new Configuration(base.getResources().getConfiguration());
        cfg.setLocale(locale(base));
        return base.createConfigurationContext(cfg);
    }

    /** Service 等非 Activity 场景取本地化字符串。 */
    public static String str(Context c, int resId) {
        return wrap(c).getString(resId);
    }

    public static String str(Context c, int resId, Object arg) {
        return wrap(c).getString(resId, arg);
    }
}
