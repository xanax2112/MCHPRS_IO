package com.dsh.mchprs;

import android.content.Context;
import android.content.SharedPreferences;

/** 工作目录的持久化（FEAT-015）。 */
public final class WorkDir {

    private static final String PREF = "settings";
    private static final String KEY = "workdir";

    private WorkDir() {
    }

    public static String get(Context c) {
        try {
            SharedPreferences sp = c.getSharedPreferences(PREF, Context.MODE_PRIVATE);
            String v = sp.getString(KEY, null);
            return (v == null || v.trim().isEmpty()) ? MainActivity.DEFAULT_WORKDIR : v;
        } catch (Exception e) {
            return MainActivity.DEFAULT_WORKDIR;
        }
    }

    public static void set(Context c, String path) {
        c.getSharedPreferences(PREF, Context.MODE_PRIVATE)
                .edit().putString(KEY, path).commit();
    }
}
