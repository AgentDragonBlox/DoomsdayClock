using System.Drawing;
using System.Drawing.Text;

namespace DoomsdayClock;

/// <summary>
/// Resolves a preferred font family name to an actually-installed one,
/// walking a fallback chain when the preferred font isn't present.
///
/// "Gill Sans Nova Ultra Bold" is a commercial Monotype font that ships with
/// some Adobe/Office installs but is not present on a stock Windows machine.
/// Rather than crash or silently draw with the default UI font, we degrade
/// gracefully through visually-similar, more commonly-installed bold
/// grotesque/humanist sans faces, ending at a guaranteed-present generic
/// family so the app *never* fails to render a wallpaper.
/// </summary>
public static class FontResolver
{
    private static readonly string[] FallbackChain =
    [
        "Gill Sans Nova Ultra Bold",
        "Gill Sans Ultra Bold",
        "Gill Sans MT Ultra Bold",
        "Gill Sans MT Bold",
        "Gill Sans MT",
        "Arial Black",
        "Segoe UI Black",
        "Segoe UI",
    ];

    // Querying installed fonts touches the OS font table. The result cannot
    // change during the lifetime of this process (which for the silent
    // "--update" path lives for well under a second), so we look it up once
    // and cache it instead of re-querying per render call.
    private static readonly Lazy<HashSet<string>> InstalledFamilyNames = new(() =>
    {
        using var installed = new InstalledFontCollection();
        return new HashSet<string>(
            installed.Families.Select(f => f.Name),
            StringComparer.OrdinalIgnoreCase);
    });

    /// <summary>
    /// Returns a <see cref="Font"/> for the given point size using the
    /// user's preferred family if installed, otherwise the closest
    /// available fallback. The caller owns the returned <see cref="Font"/>
    /// and must dispose it.
    /// </summary>
    public static Font Resolve(string preferredFamilyName, float pointSize)
    {
        var candidates = FallbackChain.AsEnumerable();

        // Try the user's exact preference first, ahead of the built-in chain,
        // in case they've installed a variant we don't know the exact name of.
        if (!string.IsNullOrWhiteSpace(preferredFamilyName))
        {
            candidates = new[] { preferredFamilyName }.Concat(candidates);
        }

        foreach (string name in candidates.Distinct(StringComparer.OrdinalIgnoreCase))
        {
            if (InstalledFamilyNames.Value.Contains(name))
            {
                // FontStyle.Bold is layered on top even for families whose
                // *name* already says "Bold"/"Ultra Bold" - GDI+ ignores the
                // flag if the face has no distinct regular weight to bold
                // from, so this is safe and keeps every fallback rung
                // visually heavy, matching the intended poster-style look.
                try
                {
                    return new Font(name, pointSize, FontStyle.Bold, GraphicsUnit.Point);
                }
                catch (ArgumentException)
                {
                    // Family reported as "installed" but failed to
                    // construct (corrupt font file, etc.) - keep falling back.
                }
            }
        }

        // Guaranteed to exist on every Windows install.
        return new Font(FontFamily.GenericSansSerif, pointSize, FontStyle.Bold, GraphicsUnit.Point);
    }
}
