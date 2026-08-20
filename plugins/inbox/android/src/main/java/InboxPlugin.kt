package dev.pombocorreio.inbox

import android.app.Activity
import android.content.ContentValues
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.OpenableColumns
import android.provider.MediaStore
import android.webkit.WebView
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import org.json.JSONArray
import java.io.File
import java.io.FileInputStream

@InvokeArg
class PublishArgs {
    lateinit var sourcePath: String
    lateinit var displayName: String
    lateinit var mimeType: String
}

@TauriPlugin
class InboxPlugin(private val activity: Activity) : Plugin(activity) {
    private val sharedFiles = mutableListOf<Pair<String, String>>()
    private var sharedText: String? = null

    override fun load(webView: WebView) {
        captureShare(activity.intent)
    }

    override fun onResume() {
        captureShare(activity.intent)
    }

    override fun onNewIntent(intent: Intent) {
        captureShare(intent)
    }

    @Command
    fun takeSharedContent(invoke: Invoke) {
        val response = synchronized(this) {
            val files = JSONArray()
            sharedFiles.forEach { (path, name) ->
                files.put(JSObject().apply {
                    put("path", path)
                    put("name", name)
                })
            }
            JSObject().apply {
                put("files", files)
                put("text", sharedText)
            }
        }
        invoke.resolve(response)
    }

    @Command
    fun clearSharedContent(invoke: Invoke) {
        synchronized(this) {
            sharedFiles.clear()
            sharedText = null
        }
        invoke.resolve()
    }

    @Command
    fun publish(invoke: Invoke) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
            invoke.reject("Public Downloads requires Android 10 or newer")
            return
        }

        val args = invoke.parseArgs(PublishArgs::class.java)
        val source = File(args.sourcePath)
        val resolver = activity.contentResolver
        val values = ContentValues().apply {
            put(MediaStore.Downloads.DISPLAY_NAME, args.displayName)
            put(MediaStore.Downloads.MIME_TYPE, args.mimeType)
            put(MediaStore.Downloads.RELATIVE_PATH, "Download/Pombo Correio")
            put(MediaStore.Downloads.IS_PENDING, 1)
        }
        val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
        if (uri == null) {
            invoke.reject("Android could not create the download")
            return
        }

        try {
            resolver.openOutputStream(uri, "w").use { output ->
                requireNotNull(output) { "Android could not open the download" }
                FileInputStream(source).use { input -> input.copyTo(output) }
            }
            values.clear()
            values.put(MediaStore.Downloads.IS_PENDING, 0)
            resolver.update(uri, values, null, null)
            invoke.resolve(JSObject().apply { put("uri", uri.toString()) })
        } catch (error: Exception) {
            resolver.delete(uri, null, null)
            invoke.reject(error.message ?: "Could not publish the download")
        }
    }

    private fun captureShare(intent: Intent?) {
        if (intent?.action != Intent.ACTION_SEND && intent?.action != Intent.ACTION_SEND_MULTIPLE) {
            return
        }

        val uris = linkedSetOf<Uri>()
        intent.clipData?.let { clip ->
            for (index in 0 until clip.itemCount) {
                clip.getItemAt(index).uri?.let(uris::add)
            }
        }
        @Suppress("DEPRECATION")
        if (intent.action == Intent.ACTION_SEND_MULTIPLE) {
            intent.getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM)?.let(uris::addAll)
        } else {
            @Suppress("DEPRECATION")
            intent.getParcelableExtra<Uri>(Intent.EXTRA_STREAM)?.let(uris::add)
        }

        synchronized(this) {
            uris.forEach { uri ->
                val item = uri.toString() to displayName(uri)
                if (sharedFiles.none { it.first == item.first }) sharedFiles.add(item)
            }
            intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()?.let { sharedText = it }
        }

        // A singleTask activity can resume more than once with the same Intent.
        // Clearing the action prevents the same share from being queued again.
        intent.action = null
    }

    private fun displayName(uri: Uri): String {
        if (uri.scheme == "content") {
            try {
                activity.contentResolver.query(
                    uri,
                    arrayOf(OpenableColumns.DISPLAY_NAME),
                    null,
                    null,
                    null,
                )?.use { cursor ->
                    if (cursor.moveToFirst()) {
                        val column = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                        if (column >= 0) cursor.getString(column)?.let { return it }
                    }
                }
            } catch (_: Exception) {}
        }
        return uri.lastPathSegment?.substringAfterLast('/') ?: "Shared file"
    }
}
