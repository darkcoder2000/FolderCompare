namespace FolderCompare.Core.Models;

public enum DiffStatus
{
    Identical,
    OnlyLeft,
    OnlyRight,
    NewerLeft,
    NewerRight,
    /// <summary>Rolled-up status of a folder whose contents are not all identical.</summary>
    Differs,
    TypeConflict,
    Error,
    /// <summary>Same timestamp (within tolerance) but size or content differs. Direction is unknown.</summary>
    SameTimeDifferentContent,
    /// <summary>Content verified identical, only the timestamp differs.</summary>
    SameContentDifferentTime,
}
