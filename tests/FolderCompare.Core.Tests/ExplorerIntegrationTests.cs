using FolderCompare.Core.Shell;
using static FolderCompare.Core.Tests.TestFs;

namespace FolderCompare.Core.Tests;

public class ExplorerIntegrationTests
{
    private const string Exe = @"C:\Program Files\Tools\FolderCompare.exe";
    private const string Dll = @"C:\Users\me\AppData\Local\FolderCompare\ShellExt\FolderCompare.ShellExt.0123abcd.dll";

    [Fact]
    public void Build_RegistersBothHandlersPerUser()
    {
        var values = ExplorerMenuLayout.Build(Exe, Dll);

        foreach (var clsid in new[] { ExplorerMenuLayout.CompareClsid, ExplorerMenuLayout.SelectLeftClsid })
        {
            Assert.Contains(new RegistryValue($@"Software\Classes\CLSID\{clsid}\InprocServer32", "", Dll), values);
            Assert.Contains(new RegistryValue($@"Software\Classes\CLSID\{clsid}\InprocServer32", "ThreadingModel", "Apartment"), values);
        }
        Assert.Contains(new RegistryValue(@"Software\FolderCompare", "ExePath", Exe), values);
        Assert.All(values, v => Assert.StartsWith(@"Software\", v.SubKey));
    }

    [Theory]
    [InlineData("*")]
    [InlineData("Directory")]
    public void Build_VerbsUseExplorerCommandHandlers(string classKey)
    {
        var values = ExplorerMenuLayout.Build(Exe, Dll);
        var compare = $@"Software\Classes\{classKey}\shell\FolderCompare.Compare";
        var select = $@"Software\Classes\{classKey}\shell\FolderCompare.SelectLeft";

        Assert.Contains(new RegistryValue(compare, "ExplorerCommandHandler", ExplorerMenuLayout.CompareClsid), values);
        Assert.Contains(new RegistryValue(select, "ExplorerCommandHandler", ExplorerMenuLayout.SelectLeftClsid), values);
        Assert.Contains(new RegistryValue(compare, "MultiSelectModel", "Player"), values);
        Assert.Contains(new RegistryValue(select, "MultiSelectModel", "Player"), values);
        Assert.Contains(new RegistryValue(compare, "Icon", $"\"{Exe}\",0"), values);
    }

    [Fact]
    public void Build_HasNoStaticLabelsOrCommands()
    {
        var values = ExplorerMenuLayout.Build(Exe, Dll);
        Assert.DoesNotContain(values, v => v.Name == "MUIVerb");
        Assert.DoesNotContain(values, v => v.SubKey.EndsWith(@"\command", StringComparison.Ordinal));
        Assert.DoesNotContain(values, v => v.Value.Contains("Compare to left", StringComparison.OrdinalIgnoreCase));
        Assert.Equal(2 * 3 + 2 * 2 * 3 + 1, values.Count);
    }

    [Fact]
    public void CompareVerbSortsBeforeSelectLeft() =>
        Assert.True(string.CompareOrdinal(ExplorerMenuLayout.CompareVerb, ExplorerMenuLayout.SelectLeftVerb) < 0);

    [Fact]
    public void VerbKeys_IncludeLegacyCompareToLeft() =>
        Assert.Equal(new[]
        {
            @"*\shell\FolderCompare.Compare", @"*\shell\FolderCompare.SelectLeft", @"*\shell\FolderCompare.CompareToLeft",
            @"Directory\shell\FolderCompare.Compare", @"Directory\shell\FolderCompare.SelectLeft", @"Directory\shell\FolderCompare.CompareToLeft",
        }, ExplorerMenuLayout.VerbKeys());

    [Fact]
    public void LegacyKeys_AreNeverWrittenByBuild()
    {
        Assert.Equal(new[]
        {
            @"Software\Classes\*\shell\FolderCompare.CompareToLeft", @"Software\Classes\*\shell\FolderCompare.SelectLeft\command",
            @"Software\Classes\Directory\shell\FolderCompare.CompareToLeft", @"Software\Classes\Directory\shell\FolderCompare.SelectLeft\command",
        }, ExplorerMenuLayout.LegacyKeys());

        var written = ExplorerMenuLayout.Build(Exe, Dll).Select(v => v.SubKey).ToHashSet();
        Assert.All(ExplorerMenuLayout.LegacyKeys(), k => Assert.DoesNotContain(k, written));
    }

    [Fact]
    public void DllFileName_ContainsHash() =>
        Assert.Equal("FolderCompare.ShellExt.0123abcd.dll", ExplorerMenuLayout.DllFileName("0123abcd"));

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
