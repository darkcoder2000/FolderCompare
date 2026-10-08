using FolderCompare.Core.Shell;
using static FolderCompare.Core.Tests.TestFs;

namespace FolderCompare.Core.Tests;

public class ExplorerIntegrationTests
{
    private const string StoreDir = @"C:\appdata\FolderCompare";

    [Fact]
    public void Store_RoundTripsAndClears()
    {
        var store = new ExplorerSelectionStore(Create(), StoreDir);
        Assert.Null(store.Read());

        store.Save(@"C:\some folder\ä file.txt");
        Assert.Equal(@"C:\some folder\ä file.txt", store.Read());

        store.Save(L);
        Assert.Equal(L, store.Read());

        store.Clear();
        Assert.Null(store.Read());
        store.Clear(); // nothing stored: no exception
    }

    [Fact]
    public void Store_EmptyFile_ReadsAsNothing()
    {
        var fs = Create();
        fs.AddFile($@"{StoreDir}\{ExplorerSelectionStore.FileName}", new System.IO.Abstractions.TestingHelpers.MockFileData("  \r\n"));
        Assert.Null(new ExplorerSelectionStore(fs, StoreDir).Read());
    }

    [Fact]
    public void Build_QuotesExeAndArgument()
    {
        const string exe = @"C:\Program Files\Tools\FolderCompare.exe";
        var values = ExplorerMenuLayout.Build(exe, null);

        Assert.Contains(new RegistryValue(@"*\shell\FolderCompare.SelectLeft\command", "",
            $"\"{exe}\" --select-left \"%1\""), values);
        Assert.Contains(new RegistryValue(@"Directory\shell\FolderCompare.CompareToLeft\command", "",
            $"\"{exe}\" --compare-to-left \"%1\""), values);
        Assert.Contains(new RegistryValue(@"Directory\shell\FolderCompare.SelectLeft", "Icon", $"\"{exe}\",0"), values);
        Assert.Equal(12, values.Count);
    }

    [Fact]
    public void Build_UsesPendingNameInCompareLabel()
    {
        var values = ExplorerMenuLayout.Build(@"C:\fc.exe", @"C:\data\project\");
        Assert.Contains(new RegistryValue(@"*\shell\FolderCompare.CompareToLeft", "MUIVerb",
            "Compare to \"project\" with FolderCompare"), values);
        Assert.Contains(new RegistryValue(@"*\shell\FolderCompare.SelectLeft", "MUIVerb",
            ExplorerMenuLayout.SelectLeftLabel), values);
    }

    [Theory]
    [InlineData(null, ExplorerMenuLayout.CompareToLeftLabel)]
    [InlineData("", ExplorerMenuLayout.CompareToLeftLabel)]
    [InlineData(@"C:\a\notes.txt", "Compare to \"notes.txt\" with FolderCompare")]
    [InlineData(@"D:\", "Compare to \"D:\\\" with FolderCompare")]
    public void CompareLabel(string? pending, string expected) =>
        Assert.Equal(expected, ExplorerMenuLayout.CompareLabel(pending));

    [Fact]
    public void VerbKeys_CoverFilesAndFolders() =>
        Assert.Equal(new[]
        {
            @"*\shell\FolderCompare.SelectLeft", @"*\shell\FolderCompare.CompareToLeft",
            @"Directory\shell\FolderCompare.SelectLeft", @"Directory\shell\FolderCompare.CompareToLeft",
        }, ExplorerMenuLayout.VerbKeys());

    [Fact]
    public void Classify()
    {
        var fs = Create();
        fs.AddFile($@"{L}\a.txt", T0);
        fs.AddFile($@"{R}\b.txt", T0);

        Assert.Equal(PairKind.BothFolders, ExplorerMenuLayout.Classify(fs, L, R));
        Assert.Equal(PairKind.BothFiles, ExplorerMenuLayout.Classify(fs, $@"{L}\a.txt", $@"{R}\b.txt"));
        Assert.Equal(PairKind.Mixed, ExplorerMenuLayout.Classify(fs, L, $@"{R}\b.txt"));
        Assert.Equal(PairKind.Missing, ExplorerMenuLayout.Classify(fs, @"C:\gone", R));
        Assert.Equal(PairKind.Missing, ExplorerMenuLayout.Classify(fs, $@"{L}\a.txt", @"C:\gone.txt"));
    }
}
