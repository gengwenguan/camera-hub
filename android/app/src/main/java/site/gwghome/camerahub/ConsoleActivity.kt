package site.gwghome.camerahub

import android.Manifest
import android.annotation.SuppressLint
import android.app.DownloadManager
import android.content.ActivityNotFoundException
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.net.http.SslError
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.view.View
import android.webkit.CookieManager
import android.webkit.PermissionRequest
import android.webkit.RenderProcessGoneDetail
import android.webkit.SslErrorHandler
import android.webkit.URLUtil
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebStorage
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import android.widget.PopupMenu
import android.widget.ProgressBar
import android.widget.TextView

class ConsoleActivity : HubActivity() {
    private lateinit var server: ServerAddress
    private lateinit var container: FrameLayout
    private lateinit var errorPanel: View
    private lateinit var progress: ProgressBar
    private var web: WebView? = null
    private var pageUrl = ""
    private var fileCallback: ValueCallback<Array<Uri>>? = null
    private var audioRequest: PermissionRequest? = null
    private var pendingDownload: Download? = null
    private var fullscreenView: View? = null
    private var fullscreenCallback: WebChromeClient.CustomViewCallback? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        server = runCatching { ServerAddress.parse(intent.getStringExtra("server").orEmpty()) }
            .getOrElse { finish(); return }
        setContentView(R.layout.activity_console)
        fitSystemBars()
        container = findViewById(R.id.web_container)
        errorPanel = findViewById(R.id.page_error)
        progress = findViewById(R.id.page_progress)
        findViewById<TextView>(R.id.console_address).text = server.url
        findViewById<View>(R.id.close_console).setOnClickListener { finish() }
        findViewById<View>(R.id.retry_page).setOnClickListener { loadPage() }
        findViewById<View>(R.id.error_open_browser).setOnClickListener { openBrowser(pageUrl) }
        findViewById<View>(R.id.console_menu).setOnClickListener(::showMenu)
        pageUrl = savedInstanceState?.getString("page")?.takeIf(server::contains)
            ?: intent.getStringExtra("page")?.takeIf { it.startsWith("/#") }
                ?.let { server.url + it } ?: "${server.url}/"
        // Publish the native session only to this server before the first navigation.
        val cookie = HubSessions.cookie(server)
        if (cookie == null) loadPage()
        else try {
            CookieManager.getInstance().setCookie(server.url,
                "$cookie; Path=/; HttpOnly; SameSite=Strict" + if (server.url.startsWith("https:")) "; Secure" else "") {
                if (!isDestroyed && !isFinishing) loadPage()
            }
        } catch (_: RuntimeException) {
            showError(getString(R.string.webview_missing))
        }
    }

    @SuppressLint("SetJavaScriptEnabled")
    private fun createWebView(): WebView? {
        val view = try {
            WebView(this)
        } catch (_: RuntimeException) {
            showError(getString(R.string.webview_missing))
            return null
        }
        view.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true
            allowFileAccess = false
            allowContentAccess = false
            mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
            mediaPlaybackRequiresUserGesture = true
            // target="_blank" links (voice studio) use this same controlled view.
            setSupportMultipleWindows(false)
        }
        CookieManager.getInstance().setAcceptCookie(true)
        CookieManager.getInstance().setAcceptThirdPartyCookies(view, false)
        view.webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                val target = request.url.toString()
                if (server.contains(target)) {
                    if (request.isForMainFrame) errorPanel.visibility = View.GONE
                    return false
                }
                if (request.isForMainFrame) {
                    if (request.hasGesture()) openBrowser(target)
                    else showError(getString(R.string.health_redirect))
                }
                return true
            }

            override fun onPageStarted(view: WebView, url: String, favicon: android.graphics.Bitmap?) {
                cancelPendingRequests()
                // Also guard renderer-driven navigation and window.open.
                if (!server.contains(url)) {
                    view.stopLoading()
                    showError(getString(R.string.health_redirect))
                    return
                }
                pageUrl = url
                // HTTP errors can arrive before onPageStarted on some WebViews.
                if (errorPanel.visibility != View.VISIBLE) progress.visibility = View.VISIBLE
            }

            override fun onPageFinished(view: WebView, url: String) {
                progress.visibility = View.GONE
            }

            override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceError) {
                if (request.isForMainFrame) showError(getString(R.string.web_error))
            }

            override fun onReceivedHttpError(
                view: WebView, request: WebResourceRequest, errorResponse: WebResourceResponse,
            ) {
                if (request.isForMainFrame) showError(getString(R.string.http_error, errorResponse.statusCode))
            }

            override fun onReceivedSslError(view: WebView, handler: SslErrorHandler, error: SslError) {
                handler.cancel()
                showError(getString(R.string.tls_error))
            }

            override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail): Boolean {
                cancelPendingRequests()
                leaveFullscreen()
                container.removeView(view)
                view.destroy()
                web = null
                showError(getString(R.string.web_error))
                return true
            }
        }
        view.webChromeClient = object : WebChromeClient() {
            override fun onProgressChanged(view: WebView, newProgress: Int) {
                progress.progress = newProgress
                if (newProgress == 100) progress.visibility = View.GONE
            }

            override fun onShowFileChooser(
                view: WebView, callback: ValueCallback<Array<Uri>>, params: FileChooserParams,
            ): Boolean {
                fileCallback?.onReceiveValue(null)
                fileCallback = callback
                try {
                    startActivityForResult(
                        Intent.createChooser(params.createIntent(), getString(R.string.choose_file)),
                        PICK_FILE,
                    )
                } catch (_: ActivityNotFoundException) {
                    fileCallback?.onReceiveValue(null)
                    fileCallback = null
                    toast(R.string.file_picker_missing)
                }
                return true
            }

            override fun onPermissionRequest(request: PermissionRequest) {
                audioRequest?.deny()
                audioRequest = null
                if (!server.contains(request.origin.toString()) ||
                    !server.contains(view.url.orEmpty()) ||
                    PermissionRequest.RESOURCE_AUDIO_CAPTURE !in request.resources
                ) {
                    request.deny()
                    return
                }
                if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) {
                    request.grant(arrayOf(PermissionRequest.RESOURCE_AUDIO_CAPTURE))
                } else {
                    audioRequest = request
                    requestPermissions(arrayOf(Manifest.permission.RECORD_AUDIO), RECORD_AUDIO)
                }
            }

            override fun onPermissionRequestCanceled(request: PermissionRequest) {
                if (audioRequest === request) audioRequest = null
            }

            override fun onShowCustomView(view: View, callback: CustomViewCallback) {
                if (fullscreenView != null) {
                    callback.onCustomViewHidden()
                    return
                }
                fullscreenView = view
                fullscreenCallback = callback
                container.addView(view, FrameLayout.LayoutParams(-1, -1))
            }

            override fun onHideCustomView() = leaveFullscreen()
        }
        view.setDownloadListener { url, userAgent, disposition, mime, _ ->
            if (url.startsWith("blob:")) {
                toast(R.string.blob_download)
            } else if (server.contains(url)) {
                val download = Download(url, userAgent, disposition, mime)
                if (Build.VERSION.SDK_INT <= 28 &&
                    checkSelfPermission(Manifest.permission.WRITE_EXTERNAL_STORAGE) != PackageManager.PERMISSION_GRANTED
                ) {
                    pendingDownload = download
                    requestPermissions(arrayOf(Manifest.permission.WRITE_EXTERNAL_STORAGE), WRITE_DOWNLOAD)
                } else {
                    enqueueDownload(download)
                }
            } else {
                openBrowser(url)
            }
        }
        container.addView(view, 0, FrameLayout.LayoutParams(-1, -1))
        web = view
        return view
    }

    private fun loadPage() {
        cancelPendingRequests()
        val view = web ?: createWebView() ?: return
        errorPanel.visibility = View.GONE
        view.loadUrl(pageUrl)
    }

    private fun showError(message: String) {
        findViewById<TextView>(R.id.page_error_message).text = message
        errorPanel.visibility = View.VISIBLE
        errorPanel.bringToFront()
        progress.visibility = View.GONE
    }

    private fun showMenu(anchor: View) {
        PopupMenu(this, anchor).apply {
            menu.add(0, 1, 0, R.string.reload)
            menu.add(0, 2, 1, R.string.open_browser)
            menu.add(0, 3, 2, R.string.clear_session).isEnabled = web != null
            setOnMenuItemClickListener {
                when (it.itemId) {
                    1 -> loadPage()
                    2 -> openBrowser(pageUrl)
                    3 -> clearSession()
                }
                true
            }
        }.show()
    }

    private fun clearSession() {
        cancelPendingRequests()
        HubSessions.clear(server)
        web?.stopLoading()
        CookieManager.getInstance().removeAllCookies {
            CookieManager.getInstance().flush()
            WebStorage.getInstance().deleteAllData()
            web?.clearCache(true)
            web?.clearHistory()
            pageUrl = "${server.url}/"
            if (!isDestroyed && !isFinishing) {
                loadPage()
                toast(R.string.session_cleared)
            }
        }
    }

    private data class Download(val url: String, val userAgent: String, val disposition: String?, val mime: String?)

    private fun enqueueDownload(download: Download) {
        try {
            val filename = URLUtil.guessFileName(download.url, download.disposition, download.mime)
                .substringAfterLast('/').substringAfterLast('\\')
            val request = DownloadManager.Request(Uri.parse(download.url))
                .setTitle(filename)
                .setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED)
                .setDestinationInExternalPublicDir(Environment.DIRECTORY_DOWNLOADS, filename)
                .addRequestHeader("User-Agent", download.userAgent)
            download.mime?.let(request::setMimeType)
            CookieManager.getInstance().getCookie(download.url)?.let { request.addRequestHeader("Cookie", it) }
            getSystemService(DownloadManager::class.java).enqueue(request)
            toast(R.string.download_started)
        } catch (_: RuntimeException) {
            toast(R.string.download_failed)
        }
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        val allowed = grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED
        when (requestCode) {
            RECORD_AUDIO -> {
                audioRequest?.let {
                    if (allowed && server.contains(it.origin.toString()) && server.contains(web?.url.orEmpty())) {
                        it.grant(arrayOf(PermissionRequest.RESOURCE_AUDIO_CAPTURE))
                    } else {
                        it.deny()
                    }
                }
                audioRequest = null
                if (!allowed) toast(R.string.microphone_denied)
            }
            WRITE_DOWNLOAD -> {
                pendingDownload?.let { if (allowed) enqueueDownload(it) }
                pendingDownload = null
                if (!allowed) toast(R.string.download_permission)
            }
        }
    }

    @Deprecated("Platform activity result API, used without an AndroidX dependency.")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == PICK_FILE) {
            val selected = WebChromeClient.FileChooserParams.parseResult(resultCode, data)
                ?.filter { it.scheme == "content" }?.toTypedArray()?.takeIf { it.isNotEmpty() }
            fileCallback?.onReceiveValue(selected)
            fileCallback = null
        }
    }

    private fun cancelPendingRequests() {
        val files = fileCallback
        val audio = audioRequest
        fileCallback = null
        audioRequest = null
        pendingDownload = null
        // A renderer may already be gone; clear references before invoking callbacks.
        runCatching { files?.onReceiveValue(null) }
        runCatching { audio?.deny() }
    }

    private fun leaveFullscreen() {
        fullscreenView?.let(container::removeView)
        fullscreenView = null
        val callback = fullscreenCallback
        fullscreenCallback = null
        runCatching { callback?.onCustomViewHidden() }
    }

    override fun handleBack() {
        when {
            fullscreenView != null -> leaveFullscreen()
            web?.canGoBack() == true -> {
                errorPanel.visibility = View.GONE
                web?.goBack()
            }
            else -> finish()
        }
    }

    override fun onResume() {
        super.onResume()
        web?.onResume()
    }

    override fun onPause() {
        web?.onPause()
        super.onPause()
    }

    override fun onSaveInstanceState(outState: Bundle) {
        outState.putString("page", pageUrl)
        super.onSaveInstanceState(outState)
    }

    override fun onDestroy() {
        cancelPendingRequests()
        leaveFullscreen()
        web?.let {
            container.removeView(it)
            it.stopLoading()
            it.destroy()
        }
        web = null
        super.onDestroy()
    }

    companion object {
        private const val PICK_FILE = 1
        private const val RECORD_AUDIO = 2
        private const val WRITE_DOWNLOAD = 3
    }
}
