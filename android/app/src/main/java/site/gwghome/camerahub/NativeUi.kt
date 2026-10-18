package site.gwghome.camerahub

import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.RippleDrawable
import android.text.InputType
import android.view.Gravity
import android.view.View
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.Switch
import android.widget.TextView
import java.util.Locale

/** Small shared style helpers for platform Views; no UI framework dependency. */
class NativeUi(val context: Context) {
    fun dp(value: Int) = (value * context.resources.displayMetrics.density).toInt()
    fun color(id: Int) = context.getColor(id)
    fun column(padding: Int = 0) = LinearLayout(context).apply {
        orientation = LinearLayout.VERTICAL
        setPadding(dp(padding), dp(padding), dp(padding), dp(padding))
    }
    fun scroll(content: View) = ScrollView(context).apply {
        isFillViewport = true
        clipToPadding = false
        addView(content)
    }
    fun shape(fill: Int, radius: Int = 8) = GradientDrawable().apply {
        setColor(color(fill))
        cornerRadius = dp(radius).toFloat()
        setStroke(dp(1), color(R.color.divider))
    }
    fun card(parent: LinearLayout): LinearLayout = column(14).also {
        it.background = shape(R.color.surface)
        parent.addView(it, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(10) })
    }
    fun text(parent: LinearLayout, value: String, size: Float = 14f, muted: Boolean = false): TextView =
        TextView(context).also {
            val displaySize = when {
                size >= 24f -> 20f
                size >= 20f -> 18f
                size >= 18f -> 16f
                else -> size
            }
            it.text = value
            it.textSize = displaySize
            it.setTextColor(color(if (muted) R.color.muted else R.color.ink))
            it.setLineSpacing(dp(2).toFloat(), 1f)
            it.setPadding(0, dp(2), 0, dp(4))
            if (size >= 18) it.typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
            parent.addView(it, LinearLayout.LayoutParams(-1, -2))
        }
    fun title(parent: LinearLayout, value: String, subtitle: String = "") {
        text(parent, value, 22f)
        if (subtitle.isNotBlank()) text(parent, subtitle, muted = true)
    }
    fun button(parent: LinearLayout, value: String, primary: Boolean = false, action: () -> Unit): Button =
        Button(context).also {
            it.text = value
            it.textSize = 14f
            it.isAllCaps = false
            it.minHeight = dp(44)
            it.minimumHeight = dp(44)
            it.setTextColor(ColorStateList(
                arrayOf(intArrayOf(-android.R.attr.state_enabled), intArrayOf()),
                intArrayOf(color(R.color.muted), color(if (primary) R.color.surface else R.color.accent)),
            ))
            it.background = RippleDrawable(ColorStateList.valueOf(color(R.color.divider)),
                shape(if (primary) R.color.accent else R.color.soft_accent, 8), null)
            it.setOnClickListener { action() }
            parent.addView(it, LinearLayout.LayoutParams(-1, -2).apply {
                topMargin = dp(6)
            })
        }
    fun navigation(parent: LinearLayout, value: String, action: () -> Unit): Button =
        Button(context).also {
            it.text = value
            it.textSize = 14f
            it.isAllCaps = false
            it.gravity = Gravity.START or Gravity.CENTER_VERTICAL
            it.minHeight = dp(48)
            it.minimumHeight = dp(48)
            it.setPadding(dp(10), 0, dp(10), 0)
            it.setTextColor(color(R.color.ink))
            it.background = RippleDrawable(
                ColorStateList.valueOf(color(R.color.soft_accent)),
                shape(R.color.surface, 0),
                null,
            )
            it.setOnClickListener { action() }
            parent.addView(it, LinearLayout.LayoutParams(-1, -2))
        }
    fun field(parent: LinearLayout, label: String, value: String = "",
              type: Int = InputType.TYPE_CLASS_TEXT, secret: Boolean = false): EditText {
        val heading = text(parent, label, 13f, true)
        return EditText(context).also {
            it.id = View.generateViewId()
            heading.labelFor = it.id
            it.isSingleLine = true
            it.inputType = if (secret) InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD else type
            // setSingleLine installs its own transformation on older Android versions.
            if (secret) it.transformationMethod = android.text.method.PasswordTransformationMethod.getInstance()
            it.textSize = 16f
            it.minHeight = dp(44)
            it.setPadding(dp(10), dp(6), dp(10), dp(6))
            it.background = shape(R.color.canvas)
            it.setTextColor(color(R.color.ink))
            it.setText(value)
            it.importantForAutofill = View.IMPORTANT_FOR_AUTOFILL_NO
            it.isSaveEnabled = !secret
            parent.addView(it, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(8) })
        }
    }
    @Suppress("UseSwitchCompatOrMaterialCode") // Deliberately use platform widgets, min API 27.
    fun toggle(parent: LinearLayout, label: String, checked: Boolean): Switch = Switch(context).also {
        it.text = label
        it.textSize = 15f
        it.setTextColor(color(R.color.ink))
        it.isChecked = checked
        it.minHeight = dp(48)
        it.switchPadding = dp(12)
        parent.addView(it, LinearLayout.LayoutParams(-1, -2))
    }
    fun row(parent: LinearLayout): LinearLayout = LinearLayout(context).also {
        it.orientation = LinearLayout.HORIZONTAL
        it.gravity = Gravity.CENTER_VERTICAL
        parent.addView(it, LinearLayout.LayoutParams(-1, -2))
    }
    fun compact(button: Button) {
        button.layoutParams = LinearLayout.LayoutParams(0, -2, 1f).apply {
            setMargins(dp(3), dp(6), dp(3), dp(6))
        }
    }

    companion object {
        fun bytes(value: Double): String {
            var number = value.coerceAtLeast(0.0)
            var unit = 0
            val units = listOf("B", "KiB", "MiB", "GiB", "TiB")
            while (number >= 1024 && unit < units.lastIndex) { number /= 1024; unit++ }
            return String.format(Locale.ROOT, "%.1f %s", number, units[unit])
        }
        fun duration(seconds: Long) = when {
            seconds >= 86400 -> "${seconds / 86400} 天 ${seconds / 3600 % 24} 小时"
            seconds >= 3600 -> "${seconds / 3600} 小时 ${seconds / 60 % 60} 分钟"
            else -> "${seconds / 60} 分钟"
        }
    }
}
