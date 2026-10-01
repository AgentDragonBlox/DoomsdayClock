using System.Drawing;

namespace DoomsdayClock;

/// <summary>
/// Pure, allocation-free math for the "fade to red as the deadline
/// approaches" colour mode. Kept isolated from the rendering code so it can
/// be reasoned about (and unit tested) independently of GDI+.
/// </summary>
public static class ColorInterpolator
{
    private static readonly Color White = Color.FromArgb(255, 255, 255, 255);
    private static readonly Color Red = Color.FromArgb(255, 235, 30, 30);

    /// <summary>
    /// Returns the wallpaper text colour for "today", given when the
    /// countdown started and when it ends.
    /// </summary>
    /// <param name="createdDate">The day the countdown was configured ("day zero", 0% elapsed).</param>
    /// <param name="targetDate">The deadline ("day zero" + full duration, 100% elapsed).</param>
    /// <param name="today">The date being rendered for.</param>
    /// <param name="mode">Whether to actually fade, or just return white.</param>
    /// <returns>
    /// White at <paramref name="createdDate"/>, red at (and past)
    /// <paramref name="targetDate"/>, linearly interpolated in between.
    /// </returns>
    public static Color GetTextColor(DateTime createdDate, DateTime targetDate, DateTime today, ColorMode mode)
    {
        if (mode == ColorMode.StaticWhite)
        {
            return White;
        }

        double totalDays = (targetDate.Date - createdDate.Date).TotalDays;
        if (totalDays <= 0)
        {
            // Target date is today or already in the past relative to when
            // the countdown was created (e.g. the user picked a date that
            // has since arrived) - treat that as "fully urgent".
            return Red;
        }

        double elapsedDays = (today.Date - createdDate.Date).TotalDays;
        double progress = Math.Clamp(elapsedDays / totalDays, 0.0, 1.0);

        return Lerp(White, Red, progress);
    }

    private static Color Lerp(Color from, Color to, double t)
    {
        int r = from.R + (int)Math.Round((to.R - from.R) * t);
        int g = from.G + (int)Math.Round((to.G - from.G) * t);
        int b = from.B + (int)Math.Round((to.B - from.B) * t);
        return Color.FromArgb(255, Clamp255(r), Clamp255(g), Clamp255(b));
    }

    private static int Clamp255(int value) => Math.Clamp(value, 0, 255);
}
