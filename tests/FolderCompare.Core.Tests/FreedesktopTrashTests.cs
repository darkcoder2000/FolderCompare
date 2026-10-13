using System.IO.Abstractions.TestingHelpers;
using FolderCompare.Core.Operations;
using static FolderCompare.Core.Tests.TestFs;

namespace FolderCompare.Core.Tests;

public class FreedesktopTrashTests
{
    private static readonly DateTime Now = new(2026, 3, 4, 5, 6, 7);
    private static readonly string Home = P(@"C:\home\me");
    private static readonly string HomeTrash = P(@"C:\home\me\.local\share\Trash");
    private static readonly string Usb = P(@"C:\media\usb");

    private static (MockFileSystem Fs, FreedesktopTrash Trash) Create()
    {
        var fs = new MockFileSystem();
        fs.Directory.CreateDirectory(Home);
        fs.Directory.CreateDirectory(Usb);
        // Everything below the USB mount point is on another file system than the home folder.
        string MountOf(string path) => path.StartsWith(Usb, StringComparison.Ordinal) ? Usb : P(@"C:\");
        return (fs, new FreedesktopTrash(fs, HomeTrash, "1000", MountOf, () => Now));
    }

    private static string Info(MockFileSystem fs, string trash, string name) =>
        fs.File.ReadAllText(Path.Join(trash, "info", name + ".trashinfo"));

    [Fact]
    public void File_InHome_GoesToHomeTrashWithInfo()
    {
        var (fs, trash) = Create();
        var file = P(@"C:\home\me\docs\a b.txt");
        fs.AddFile(file, new MockFileData("x"));

        trash.DeleteFile(file);

        Assert.False(fs.File.Exists(file));
        Assert.Equal("x", fs.File.ReadAllText(Path.Join(HomeTrash, "files", "a b.txt")));
        Assert.Equal("[Trash Info]\n" +
                     $"Path={FreedesktopTrash.Encode(file)}\n" +
                     "DeletionDate=2026-03-04T05:06:07\n", Info(fs, HomeTrash, "a b.txt"));
    }

    [Fact]
    public void NameClash_GetsNumberedName()
    {
        var (fs, trash) = Create();
        var first = P(@"C:\home\me\one\same.txt");
        var second = P(@"C:\home\me\two\same.txt");
        fs.AddFile(first, new MockFileData("1"));
        fs.AddFile(second, new MockFileData("2"));

        trash.DeleteFile(first);
        trash.DeleteFile(second);

        Assert.Equal("1", fs.File.ReadAllText(Path.Join(HomeTrash, "files", "same.txt")));
        Assert.Equal("2", fs.File.ReadAllText(Path.Join(HomeTrash, "files", "same.txt.2")));
        Assert.Contains(FreedesktopTrash.Encode(second), Info(fs, HomeTrash, "same.txt.2"));
    }

    [Fact]
    public void Directory_IsMovedWithContent()
    {
        var (fs, trash) = Create();
        var dir = P(@"C:\home\me\project");
        fs.AddFile(Path.Join(dir, "sub", "f.txt"), new MockFileData("f"));

        trash.DeleteDirectory(dir);

        Assert.False(fs.Directory.Exists(dir));
        Assert.Equal("f", fs.File.ReadAllText(Path.Join(HomeTrash, "files", "project", "sub", "f.txt")));
        Assert.True(fs.File.Exists(Path.Join(HomeTrash, "info", "project.trashinfo")));
    }

    [Fact]
    public void OtherMount_UsesTopdirTrashWithRelativePath()
    {
        var (fs, trash) = Create();
        var file = P(@"C:\media\usb\photos\p.jpg");
        fs.AddFile(file, new MockFileData("jpg"));

        Assert.True(trash.CanRecycle(file));
        trash.DeleteFile(file);

        var topTrash = Path.Join(Usb, ".Trash-1000");
        Assert.Equal("jpg", fs.File.ReadAllText(Path.Join(topTrash, "files", "p.jpg")));
        Assert.Contains($"Path={FreedesktopTrash.Encode(Path.Join("photos", "p.jpg"))}\n", Info(fs, topTrash, "p.jpg"));
        Assert.False(fs.Directory.Exists(HomeTrash));
    }

    [Fact]
    public void FailedMove_RemovesTheReservedInfoFile()
    {
        var (fs, trash) = Create();
        var missing = P(@"C:\home\me\gone.txt");

        Assert.ThrowsAny<IOException>(() => trash.DeleteFile(missing));
        Assert.False(fs.File.Exists(Path.Join(HomeTrash, "info", "gone.txt.trashinfo")));
    }

    [Theory]
    [InlineData("/home/me/a b.txt", "/home/me/a%20b.txt")]
    [InlineData("/x/über%.txt", "/x/%C3%BCber%25.txt")]
    [InlineData("rel/name-_.~", "rel/name-_.~")]
    public void Encode_IsFileUrlStyle(string path, string expected) => Assert.Equal(expected, FreedesktopTrash.Encode(path));

    [Fact]
    public void MountPointOf_PicksTheLongestMatchingMount()
    {
        string[] lines =
        [
            "22 1 8:1 / / rw,relatime shared:1 - ext4 /dev/sda1 rw",
            "40 22 8:17 / /media/usb rw,nosuid shared:20 - vfat /dev/sdb1 rw",
            "41 22 8:18 / /media/my\\040disk rw shared:21 - ext4 /dev/sdb2 rw",
        ];
        Assert.Equal("/media/usb", FreedesktopTrash.MountPointOf("/media/usb/a/b.txt", lines));
        Assert.Equal("/", FreedesktopTrash.MountPointOf("/media/usb2/x", lines));
        Assert.Equal("/media/my disk", FreedesktopTrash.MountPointOf("/media/my disk/f", lines));
        Assert.Equal("/", FreedesktopTrash.MountPointOf("/home/me", lines));
    }
}
