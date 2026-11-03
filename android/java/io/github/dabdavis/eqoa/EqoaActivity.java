package io.github.dabdavis.eqoa;

import android.app.AlertDialog;
import android.app.NativeActivity;
import android.content.Intent;
import android.content.pm.ApplicationInfo;
import android.database.Cursor;
import android.hardware.input.InputManager;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;
import android.system.Os;
import android.text.Editable;
import android.text.InputType;
import android.text.Selection;
import android.view.InputDevice;
import android.view.InputEvent;
import android.view.KeyCharacterMap;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewGroup;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.InputMethodManager;

import java.io.File;

/**
 * The game's activity: Android's NativeActivity, which loads the game
 * (libeqoa_android.so) and runs its android_main on a thread of its own, and
 * what the game needs of Java besides (crates/eqoa/src/android.rs is the
 * other half):
 *
 * - every input event, handed over as it comes (native*): the window's input
 *   queue is not given to the native side (winit takes a controller's sticks
 *   for a touch), so touches, keys and controllers all come through
 *   dispatch* here;
 * - the device's keyboard for the game's edit boxes (wantText), what is typed
 *   handed over as characters;
 * - the disc image: picked in Android's own picker (pickDisc), handed over as
 *   an open file, and kept for the next start (keepDisc), when it is opened
 *   again before the game starts ($EQOA_ISO);
 * - where the game keeps its files: the app's own folders ($EQOA_CONFIG,
 *   $EQOA_CACHE).
 */
public class EqoaActivity extends NativeActivity {
    static {
        // the game itself, loaded here too so its native* methods resolve
        // (NativeActivity loads it again by its meta-data, the same library)
        System.loadLibrary("eqoa_android");
    }

    private static final String TITLE = "EverQuest Online Adventures";
    private static final int PICK_DISC = 1;
    private static final String PREFS = "eqoa";
    private static final String KEPT_DISC = "disc";

    /** The disc handed over last, kept once the game plays it (keepDisc). */
    private Uri picked;
    /** Why the disc kept from the last time could not be opened. */
    private String keptNote;
    private TextSink text;
    private boolean wantText;
    /** A debuggable build's trace of the input handed over (the start's extra EQOA_INPUT_TRACE=1), to logcat. */
    static boolean trace;

    static void trace(String s) {
        if (trace) {
            android.util.Log.d("eqoa-input", s);
        }
    }

    // android.rs
    static native void nativeTouch(int action, int id, float x, float y);
    static native void nativeKey(int code, int unicode, boolean down, int repeat, boolean soft);
    static native void nativeText(int back, String text);
    static native void nativePadKey(int device, String label, int code, boolean down);
    static native void nativePadAxes(int device, String label, float lx, float ly, float rx, float ry, float hatX, float hatY, float lt, float rt);
    static native void nativePadGone(int device);
    static native void nativeDisc(String path);
    static native void nativeDiscAnswer(boolean yes);
    static native void nativePaused(boolean paused);
    static native void nativeKeyboard(int height);

    @Override
    protected void onCreate(Bundle state) {
        // where the game keeps its things: the app's own folders
        setenv("EQOA_CONFIG", new File(getFilesDir(), "config").getPath());
        setenv("EQOA_CACHE", new File(getCacheDir(), "dnas").getPath());
        setenv("HOME", getFilesDir().getPath());
        setenv("TMPDIR", getCacheDir().getPath());
        setenv("RUST_BACKTRACE", "1");
        // a debuggable build (build.sh --debug) takes the game's EQOA_*
        // settings from the start's extras, as a desktop's run takes them from
        // its environment (a test: am start -e EQOA_AUTH_PORT 19735 ...)
        Bundle extras = getIntent() != null ? getIntent().getExtras() : null;
        if ((getApplicationInfo().flags & ApplicationInfo.FLAG_DEBUGGABLE) != 0 && extras != null) {
            for (String k : extras.keySet()) {
                if (k.startsWith("EQOA_")) {
                    setenv(k, String.valueOf(extras.get(k)));
                }
            }
            trace = "1".equals(String.valueOf(extras.get("EQOA_INPUT_TRACE")));
        }
        // the disc played last time, opened again for the game ($EQOA_ISO)
        openKeptDisc();
        super.onCreate(state);
        // every input event comes to dispatch* (android.rs says why)
        getWindow().takeInputQueue(null);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        text = new TextSink(this);
        addContentView(text, new ViewGroup.LayoutParams(1, 1));
        watchKeyboard();
        immersive();
    }

    /**
     * How much of the window's bottom the device's keyboard covers, told to
     * the game as it changes: the game draws above it (android.rs), as a
     * browser shrinks a page for it. Android 11 and later say it in the
     * window's insets; before, it is the window less what can be seen.
     */
    private int keyboardHeight = -1;

    private void watchKeyboard() {
        final View decor = getWindow().getDecorView();
        if (Build.VERSION.SDK_INT >= 30) {
            decor.setOnApplyWindowInsetsListener((v, insets) -> {
                keyboard(insets.getInsets(WindowInsets.Type.ime()).bottom);
                return v.onApplyWindowInsets(insets);
            });
        } else {
            decor.getViewTreeObserver().addOnGlobalLayoutListener(() -> {
                android.graphics.Rect seen = new android.graphics.Rect();
                decor.getWindowVisibleDisplayFrame(seen);
                int covered = decor.getHeight() - seen.bottom;
                // a navigation bar is not the keyboard
                keyboard(covered > decor.getHeight() / 8 ? covered : 0);
            });
        }
    }

    private void keyboard(int height) {
        if (height != keyboardHeight) {
            keyboardHeight = height;
            nativeKeyboard(height);
        }
    }

    private static void setenv(String name, String value) {
        try {
            Os.setenv(name, value, true);
        } catch (Exception e) {
            android.util.Log.w("eqoa", "setenv " + name + ": " + e);
        }
    }

    @Override
    protected void onResume() {
        super.onResume();
        nativePaused(false);
        getSystemService(InputManager.class).registerInputDeviceListener(devices, null);
        immersive();
    }

    @Override
    protected void onPause() {
        nativePaused(true);
        getSystemService(InputManager.class).unregisterInputDeviceListener(devices);
        super.onPause();
    }

    /**
     * The activity gone: the game's process with it. The game's event loop
     * is made once a process, so a new start must be a new process.
     */
    @Override
    protected void onDestroy() {
        super.onDestroy();
        System.exit(0);
    }

    @Override
    public void onWindowFocusChanged(boolean has) {
        super.onWindowFocusChanged(has);
        if (has) {
            // the keyboard as the game wants it: back for an edit box that
            // still has the keys, never for none (Android puts it back for
            // the view it last typed in)
            keyboardShown(wantText);
            immersive();
        }
    }

    /** The whole screen the game's: the system's bars hidden, a swipe shows them a moment. */
    private void immersive() {
        if (Build.VERSION.SDK_INT >= 30) {
            getWindow().setDecorFitsSystemWindows(false);
            WindowInsetsController c = getWindow().getInsetsController();
            if (c != null) {
                c.hide(WindowInsets.Type.systemBars());
                c.setSystemBarsBehavior(WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
            }
        } else {
            getWindow().getDecorView().setSystemUiVisibility(View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY | View.SYSTEM_UI_FLAG_FULLSCREEN | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                    | View.SYSTEM_UI_FLAG_LAYOUT_STABLE | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN);
        }
    }

    // ---- input

    @Override
    public boolean dispatchTouchEvent(MotionEvent e) {
        switch (e.getActionMasked()) {
            case MotionEvent.ACTION_DOWN:
            case MotionEvent.ACTION_POINTER_DOWN: {
                int i = e.getActionIndex();
                nativeTouch(0, e.getPointerId(i), e.getX(i), e.getY(i));
                break;
            }
            case MotionEvent.ACTION_MOVE:
                for (int i = 0; i < e.getPointerCount(); i++) {
                    nativeTouch(1, e.getPointerId(i), e.getX(i), e.getY(i));
                }
                break;
            case MotionEvent.ACTION_UP:
            case MotionEvent.ACTION_POINTER_UP: {
                int i = e.getActionIndex();
                nativeTouch(2, e.getPointerId(i), e.getX(i), e.getY(i));
                break;
            }
            case MotionEvent.ACTION_CANCEL:
                for (int i = 0; i < e.getPointerCount(); i++) {
                    nativeTouch(3, e.getPointerId(i), e.getX(i), e.getY(i));
                }
                break;
            default:
                break;
        }
        return true;
    }

    private static boolean fromPad(InputEvent e) {
        int s = e.getSource();
        return (s & InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD || (s & InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK;
    }

    private static String label(InputDevice d) {
        return d != null ? d.getName() : "controller";
    }

    @Override
    public boolean dispatchGenericMotionEvent(MotionEvent e) {
        // a controller's sticks, hat and triggers, all of them each time
        if ((e.getSource() & InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK && e.getActionMasked() == MotionEvent.ACTION_MOVE) {
            InputDevice d = e.getDevice();
            // the right stick on Z and RZ, or on RX and RY where there is no RZ
            boolean rz = d == null || d.getMotionRange(MotionEvent.AXIS_RZ, e.getSource()) != null;
            nativePadAxes(e.getDeviceId(), label(d),
                    e.getAxisValue(MotionEvent.AXIS_X), e.getAxisValue(MotionEvent.AXIS_Y),
                    e.getAxisValue(rz ? MotionEvent.AXIS_Z : MotionEvent.AXIS_RX), e.getAxisValue(rz ? MotionEvent.AXIS_RZ : MotionEvent.AXIS_RY),
                    e.getAxisValue(MotionEvent.AXIS_HAT_X), e.getAxisValue(MotionEvent.AXIS_HAT_Y),
                    Math.max(e.getAxisValue(MotionEvent.AXIS_LTRIGGER), e.getAxisValue(MotionEvent.AXIS_BRAKE)),
                    Math.max(e.getAxisValue(MotionEvent.AXIS_RTRIGGER), e.getAxisValue(MotionEvent.AXIS_GAS)));
        }
        // a mouse's hover and wheel: nothing yet
        return true;
    }

    /** The system's keys: the volume, the call keys, the camera, the assistant. */
    private static boolean systems(int code) {
        switch (code) {
            case KeyEvent.KEYCODE_VOLUME_UP:
            case KeyEvent.KEYCODE_VOLUME_DOWN:
            case KeyEvent.KEYCODE_VOLUME_MUTE:
            case KeyEvent.KEYCODE_HEADSETHOOK:
            case KeyEvent.KEYCODE_CAMERA:
            case KeyEvent.KEYCODE_ASSIST:
            case KeyEvent.KEYCODE_SEARCH:
                return true;
            default:
                return false;
        }
    }

    @Override
    public boolean dispatchKeyEvent(KeyEvent e) {
        if (systems(e.getKeyCode())) {
            return super.dispatchKeyEvent(e);
        }
        key(e);
        return true;
    }

    /** A key that is not the system's, handed over: a controller's to its state, any other as a key. */
    void key(KeyEvent e) {
        int code = e.getKeyCode();
        trace("key " + e);
        // characters typed at once (a keyboard's string)
        if (e.getAction() == KeyEvent.ACTION_MULTIPLE) {
            String s = e.getCharacters();
            if (s != null) {
                nativeText(0, s);
            }
            return;
        }
        boolean down = e.getAction() == KeyEvent.ACTION_DOWN;
        // a controller's buttons and d-pad: the controller's state
        if (fromPad(e) && (KeyEvent.isGamepadButton(code) || code == KeyEvent.KEYCODE_DPAD_UP || code == KeyEvent.KEYCODE_DPAD_DOWN
                || code == KeyEvent.KEYCODE_DPAD_LEFT || code == KeyEvent.KEYCODE_DPAD_RIGHT)) {
            nativePadKey(e.getDeviceId(), label(e.getDevice()), code, down);
            return;
        }
        // Back: the game's Escape
        if (code == KeyEvent.KEYCODE_BACK) {
            code = KeyEvent.KEYCODE_ESCAPE;
        }
        boolean soft = (e.getFlags() & KeyEvent.FLAG_SOFT_KEYBOARD) != 0 || e.getDeviceId() == KeyCharacterMap.VIRTUAL_KEYBOARD;
        nativeKey(code, e.getUnicodeChar(), down, e.getRepeatCount(), soft);
    }

    private final InputManager.InputDeviceListener devices = new InputManager.InputDeviceListener() {
        @Override
        public void onInputDeviceAdded(int id) {}

        @Override
        public void onInputDeviceChanged(int id) {}

        @Override
        public void onInputDeviceRemoved(int id) {
            nativePadGone(id);
        }
    };

    // ---- the device's keyboard

    /** The game wants text, or no longer does (android::want_text): the device's keyboard up for it, or down. */
    public void wantText(final boolean on) {
        runOnUiThread(() -> {
            wantText = on;
            keyboardShown(on);
        });
    }

    /**
     * The device's keyboard up, typing into the text sink, or down; the sink
     * takes the focus only while it is up, so nothing brings the keyboard
     * back for it after (the app's return from the back does).
     */
    private void keyboardShown(boolean on) {
        InputMethodManager imm = getSystemService(InputMethodManager.class);
        if (on) {
            text.setFocusable(true);
            text.setFocusableInTouchMode(true);
            text.requestFocus();
            imm.restartInput(text);
            imm.showSoftInput(text, 0);
        } else {
            imm.hideSoftInputFromWindow(text.getWindowToken(), 0);
            text.clearFocus();
            text.setFocusable(false);
            text.setFocusableInTouchMode(false);
            immersive();
        }
    }

    /**
     * Where the device's keyboard types: no view of its own to see, an editor
     * holding a few spaces between keystrokes (as the page's hidden input
     * does), so that whatever the keyboard does to it, what changed is all
     * that is read and handed over (Backspace for each character taken away,
     * then the ones put in).
     */
    static final class TextSink extends View {
        private final EqoaActivity activity;

        TextSink(EqoaActivity a) {
            super(a);
            activity = a;
            // focusable only while the game wants text (keyboardShown)
            setFocusable(false);
        }

        /**
         * A key while the device's keyboard is up, before the keyboard sees
         * it (a hardware keyboard's, a controller's): the game's at once. Seen
         * by the device's keyboard first, one that came while it was still
         * starting was lost; the device's keyboard's own come through its
         * connection (Typed).
         */
        @Override
        public boolean dispatchKeyEventPreIme(KeyEvent e) {
            if (systems(e.getKeyCode())) {
                return super.dispatchKeyEventPreIme(e);
            }
            activity.key(e);
            return true;
        }

        @Override
        public boolean onCheckIsTextEditor() {
            return true;
        }

        @Override
        public InputConnection onCreateInputConnection(EditorInfo out) {
            // no suggestions, no corrections: each key as it is typed
            out.inputType = InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS;
            out.imeOptions = EditorInfo.IME_FLAG_NO_FULLSCREEN | EditorInfo.IME_FLAG_NO_EXTRACT_UI | EditorInfo.IME_ACTION_DONE;
            out.initialSelStart = Typed.REST.length();
            out.initialSelEnd = Typed.REST.length();
            return new Typed(this);
        }
    }

    static final class Typed extends BaseInputConnection {
        static final String REST = "    ";
        private String last = REST;

        Typed(View v) {
            super(v, true);
            rest();
        }

        private void rest() {
            Editable e = getEditable();
            e.replace(0, e.length(), REST);
            Selection.setSelection(e, REST.length());
            last = REST;
        }

        /** What changed since the last, handed over; at rest again unless a word is still being put together. */
        private void changed() {
            Editable e = getEditable();
            String v = e.toString();
            trace("typed '" + v + "' after '" + last + "', composing " + getComposingSpanStart(e));
            int same = 0;
            while (same < v.length() && same < last.length() && v.charAt(same) == last.charAt(same)) {
                same++;
            }
            int back = last.length() - same;
            String put = v.substring(same);
            if (back > 0 || !put.isEmpty()) {
                nativeText(back, put);
            }
            last = v;
            if (getComposingSpanStart(e) < 0) {
                rest();
            }
        }

        @Override
        public boolean commitText(CharSequence t, int pos) {
            boolean r = super.commitText(t, pos);
            changed();
            return r;
        }

        @Override
        public boolean setComposingText(CharSequence t, int pos) {
            boolean r = super.setComposingText(t, pos);
            changed();
            return r;
        }

        @Override
        public boolean finishComposingText() {
            boolean r = super.finishComposingText();
            changed();
            return r;
        }

        @Override
        public boolean deleteSurroundingText(int before, int after) {
            boolean r = super.deleteSurroundingText(before, after);
            changed();
            return r;
        }

        /** The keyboard's own keys (Enter, a Backspace with nothing left, the arrows): as keys. */
        @Override
        public boolean sendKeyEvent(KeyEvent e) {
            trace("keyboard's key " + e);
            if (e.getAction() == KeyEvent.ACTION_DOWN || e.getAction() == KeyEvent.ACTION_UP) {
                nativeKey(e.getKeyCode(), e.getUnicodeChar(), e.getAction() == KeyEvent.ACTION_DOWN, e.getRepeatCount(), true);
            }
            return true;
        }

        /** Done (the keyboard's Enter): the game's Enter. */
        @Override
        public boolean performEditorAction(int action) {
            nativeKey(KeyEvent.KEYCODE_ENTER, 0, true, 0, true);
            nativeKey(KeyEvent.KEYCODE_ENTER, 0, false, 0, true);
            return true;
        }
    }

    // ---- the disc image

    /** The disc kept from the last time, opened again: $EQOA_ISO names it for the game. */
    private void openKeptDisc() {
        String kept = getSharedPreferences(PREFS, MODE_PRIVATE).getString(KEPT_DISC, null);
        if (kept == null) {
            return;
        }
        Uri u = Uri.parse(kept);
        try {
            setenv("EQOA_ISO", open(u));
            picked = u;
        } catch (Exception e) {
            keptNote = "The disc image chosen last time cannot be used (" + e.getMessage() + ").";
        }
    }

    /** The image at `u` opened: its descriptor as a path the game opens it by (iso::open_file). */
    private String open(Uri u) throws Exception {
        ParcelFileDescriptor pfd = getContentResolver().openFileDescriptor(u, "r");
        if (pfd == null) {
            throw new java.io.FileNotFoundException("it is not there any more");
        }
        return "/proc/self/fd/" + pfd.detachFd();
    }

    private String name(Uri u) {
        try (Cursor c = getContentResolver().query(u, new String[] {OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (c != null && c.moveToFirst()) {
                return c.getString(0);
            }
        } catch (Exception e) {
            // the name only
        }
        return "The file chosen";
    }

    /** The game asks for its disc (android::pick_disc); `note` says why it asks again. */
    public void pickDisc(final String note) {
        runOnUiThread(() -> {
            String why = note != null ? note : keptNote;
            keptNote = null;
            new AlertDialog.Builder(this)
                    .setTitle(TITLE)
                    .setMessage((why != null ? why + "\n\n" : "") + "Choose your EverQuest Online Adventures: Frontiers disc image (SLUS-20744), an .iso file on this device.")
                    .setCancelable(false)
                    .setPositiveButton("Choose", (d, w) -> openPicker())
                    .setNegativeButton("Quit", (d, w) -> nativeDisc(null))
                    .show();
        });
    }

    private void openPicker() {
        Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT);
        i.addCategory(Intent.CATEGORY_OPENABLE);
        // a disc image has no MIME type of its own
        i.setType("*/*");
        i.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
        try {
            startActivityForResult(i, PICK_DISC);
        } catch (Exception e) {
            pickDisc("This device has no file picker (" + e.getMessage() + ").");
        }
    }

    @Override
    protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request != PICK_DISC) {
            return;
        }
        Uri u = result == RESULT_OK && data != null ? data.getData() : null;
        if (u == null) {
            // back from the picker with nothing: asked again
            pickDisc(null);
            return;
        }
        try {
            getContentResolver().takePersistableUriPermission(u, Intent.FLAG_GRANT_READ_URI_PERMISSION);
        } catch (SecurityException e) {
            // this run's only: asked for again next time
        }
        try {
            String path = open(u);
            picked = u;
            nativeDisc(path);
        } catch (Exception e) {
            pickDisc(name(u) + " cannot be opened (" + e.getMessage() + ").");
        }
    }

    /** The game plays the disc handed over (android::keep_disc): kept for the next start. */
    public void keepDisc() {
        final Uri u = picked;
        if (u != null) {
            getSharedPreferences(PREFS, MODE_PRIVATE).edit().putString(KEPT_DISC, u.toString()).apply();
        }
    }

    /** A disc that is not Frontiers (android::confirm_disc): play with it anyway, or choose another. */
    public void confirmDisc(final String line) {
        runOnUiThread(() -> new AlertDialog.Builder(this)
                .setTitle(TITLE)
                .setMessage(line)
                .setCancelable(false)
                .setPositiveButton("Play anyway", (d, w) -> nativeDiscAnswer(true))
                .setNegativeButton("Choose another", (d, w) -> nativeDiscAnswer(false))
                .show());
    }

    /** The run ended (android_main's end): the activity closes, and the process after it. */
    public void exitGame() {
        runOnUiThread(this::finishAndRemoveTask);
    }
}
