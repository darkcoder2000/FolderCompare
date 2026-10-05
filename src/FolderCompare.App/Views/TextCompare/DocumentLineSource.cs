using FolderCompare.Core.TextDiff;
using ICSharpCode.AvalonEdit.Document;

namespace FolderCompare.App.Views.TextCompare;

internal sealed class DocumentLineSource : ILineSource
{
    private readonly TextDocument _document;

    public DocumentLineSource(TextDocument document) => _document = document;

    public int LineCount => _document.LineCount;
    public int TextLength => _document.TextLength;
    public int LineStart(int line) => _document.GetLineByNumber(line + 1).Offset;
    public int LineEnd(int line) => _document.GetLineByNumber(line + 1).EndOffset;
    public string GetText(int offset, int length) => _document.GetText(offset, length);
}
