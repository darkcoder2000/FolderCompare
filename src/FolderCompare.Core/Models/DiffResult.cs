namespace FolderCompare.Core.Models;

public sealed class DiffResult
{
    public DiffResult(string leftRoot, string rightRoot, CompareOptions options, DiffNode root)
    {
        LeftRoot = leftRoot;
        RightRoot = rightRoot;
        Options = options;
        Root = root;
    }

    public string LeftRoot { get; }
    public string RightRoot { get; }
    public CompareOptions Options { get; }
    public DiffNode Root { get; }

    public string RootFor(Side side) => side == Side.Left ? LeftRoot : RightRoot;
}
