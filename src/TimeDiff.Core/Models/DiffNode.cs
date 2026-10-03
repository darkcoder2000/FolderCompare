namespace TimeDiff.Core.Models;

/// <summary>
/// One relative path in the comparison tree. Mutable so that individual nodes can be
/// refreshed in place after file operations without rescanning the whole tree.
/// </summary>
public sealed class DiffNode
{
    public DiffNode(string relativePath, string name, bool isDirectory)
    {
        RelativePath = relativePath;
        Name = name;
        IsDirectory = isDirectory;
    }

    public string RelativePath { get; }
    public string Name { get; }
    public bool IsDirectory { get; }
    public FileInfoSnapshot? Left { get; set; }
    public FileInfoSnapshot? Right { get; set; }
    public DiffStatus Status { get; set; }
    public string? ErrorMessage { get; set; }
    public List<DiffNode> Children { get; } = new();
    public DiffNode? Parent { get; internal set; }

    /// <summary>File size, or the sum of all file sizes below a folder, on the left side.</summary>
    public long LeftTotalSize { get; set; }
    public long RightTotalSize { get; set; }

    public bool ExistsLeft => Left is not null;
    public bool ExistsRight => Right is not null;

    public FileInfoSnapshot? Get(Side side) => side == Side.Left ? Left : Right;

    public IEnumerable<DiffNode> Descendants()
    {
        var stack = new Stack<DiffNode>();
        for (int i = Children.Count - 1; i >= 0; i--) stack.Push(Children[i]);
        while (stack.Count > 0)
        {
            var n = stack.Pop();
            yield return n;
            for (int i = n.Children.Count - 1; i >= 0; i--) stack.Push(n.Children[i]);
        }
    }

    public bool IsDescendantOf(DiffNode ancestor)
    {
        for (var p = Parent; p is not null; p = p.Parent)
            if (ReferenceEquals(p, ancestor)) return true;
        return false;
    }

    public override string ToString() => $"{RelativePath} [{Status}]";
}

public enum Side
{
    Left,
    Right,
}
