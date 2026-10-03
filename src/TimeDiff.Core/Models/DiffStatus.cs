namespace TimeDiff.Core.Models;

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
}
