package org.servo.servoshell

import android.content.Context
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.Typeface
import android.os.SystemClock
import android.view.View
import kotlin.math.min

/** Native branding, independent of the game's assets and launcher icon. */
class RovesSplashView(context: Context) : View(context) {
    private val icon = context.assets.open("roves-brand/servo_1024.png").use {
        BitmapFactory.decodeStream(it)
    }
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
    private val font = Typeface.createFromAsset(context.assets, "roves-brand/fonts/MetalMania-Regular.ttf")
    private val started = SystemClock.uptimeMillis()

    init {
        setBackgroundColor(Color.BLACK)
        isClickable = true
        contentDescription = "Roves"
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        val density = resources.displayMetrics.density
        val scale = min(density, width / 400f)
        paint.typeface = font
        paint.textSize = 64f * scale
        paint.color = Color.WHITE
        val iconSize = 120f * scale
        val gap = 8f * scale
        val textWidth = paint.measureText("Roves")
        val left = (width - iconSize - gap - textWidth) / 2f
        val top = height / 2f - iconSize / 2f - 20f * scale
        canvas.drawBitmap(icon, null, RectF(left, top, left + iconSize, top + iconSize), paint)
        val metrics = paint.fontMetrics
        canvas.drawText("Roves", left + iconSize + gap,
            top + iconSize / 2f - (metrics.ascent + metrics.descent) / 2f, paint)
        val barWidth = 240f * scale
        val barLeft = (width - barWidth) / 2f
        val barTop = top + iconSize + 32f * scale
        paint.color = Color.DKGRAY
        canvas.drawRect(barLeft, barTop, barLeft + barWidth, barTop + 4f * scale, paint)
        val phase = ((SystemClock.uptimeMillis() - started) % 1400L) / 1400f
        val highlight = barWidth * 0.25f
        val x = barLeft - highlight + phase * (barWidth + highlight)
        canvas.save()
        canvas.clipRect(barLeft, barTop, barLeft + barWidth, barTop + 4f * scale)
        paint.color = Color.WHITE
        canvas.drawRect(x, barTop, x + highlight, barTop + 4f * scale, paint)
        canvas.restore()
        postInvalidateOnAnimation()
    }
}
