using System.Text;
using System.Text.RegularExpressions;

namespace TimeDiff.Core.Comparison;

/// <summary>
/// Matches semicolon-separated glob patterns (<c>*</c>, <c>?</c>, <c>**</c>) case-insensitively.
/// Patterns without a path separator match the item name; patterns with one match the relative path.
/// </summary>
public sealed class PathPatternMatcher
{
    private readonly Pattern[] _include;
    private readonly Pattern[] _exclude;

    public PathPatternMatcher(string? includePatterns, string? excludePatterns)
    {
        _include = Parse(includePatterns);
        _exclude = Parse(excludePatterns);
    }

    public bool IsExcluded(string name, string relativePath) => _exclude.Any(p => p.IsMatch(name, relativePath));

    /// <summary>Include patterns only restrict files; folders are always traversed.</summary>
    public bool IsIncludedFile(string name, string relativePath) =>
        _include.Length == 0 || _include.Any(p => p.IsMatch(name, relativePath));

    private static Pattern[] Parse(string? patterns)
    {
        if (string.IsNullOrWhiteSpace(patterns)) return [];
        return patterns
            .Split(';', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries)
            .Select(p => p.Replace('/', '\\').Trim('\\'))
            .Where(p => p.Length > 0)
            .Select(p => new Pattern(p))
            .ToArray();
    }

    private sealed class Pattern
    {
        private readonly Regex _regex;
        private readonly bool _matchPath;

        public Pattern(string glob)
        {
            _matchPath = glob.Contains('\\');
            _regex = new Regex(ToRegex(glob), RegexOptions.IgnoreCase | RegexOptions.CultureInvariant);
        }

        public bool IsMatch(string name, string relativePath) => _regex.IsMatch(_matchPath ? relativePath : name);

        private static string ToRegex(string glob)
        {
            var sb = new StringBuilder("^");
            for (int i = 0; i < glob.Length; i++)
            {
                char c = glob[i];
                if (c == '*')
                {
                    if (i + 1 < glob.Length && glob[i + 1] == '*')
                    {
                        sb.Append(".*");
                        i++;
                    }
                    else
                    {
                        sb.Append(@"[^\\]*");
                    }
                }
                else if (c == '?')
                {
                    sb.Append(@"[^\\]");
                }
                else
                {
                    sb.Append(Regex.Escape(c.ToString()));
                }
            }
            return sb.Append('$').ToString();
        }
    }
}
