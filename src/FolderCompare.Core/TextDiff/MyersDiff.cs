namespace FolderCompare.Core.TextDiff;

/// <summary>
/// Myers' O(ND) difference algorithm (linear-space, middle-snake variant) on integer sequences.
/// Marks every element of <c>a</c> and <c>b</c> that is not part of the common subsequence.
/// </summary>
public static class MyersDiff
{
    /// <summary>Above this amount of work per middle-snake search the split falls back to the midpoint.
    /// The result is still a valid (just not minimal) edit script.</summary>
    private const long MaxWorkPerSnake = 20_000_000;

    public static (bool[] ModifiedA, bool[] ModifiedB) Compute(int[] a, int[] b)
    {
        var modA = new bool[a.Length];
        var modB = new bool[b.Length];

        // Elements that never occur on the other side can never match. Marking them up front keeps D small
        // for files that have little in common, which is where Myers gets slow.
        var inB = new HashSet<int>(b);
        var inA = new HashSet<int>(a);
        var idxA = new List<int>(a.Length);
        var idxB = new List<int>(b.Length);
        for (int i = 0; i < a.Length; i++)
            if (inB.Contains(a[i])) idxA.Add(i); else modA[i] = true;
        for (int j = 0; j < b.Length; j++)
            if (inA.Contains(b[j])) idxB.Add(j); else modB[j] = true;

        var ca = idxA.Select(i => a[i]).ToArray();
        var cb = idxB.Select(j => b[j]).ToArray();
        var cModA = new bool[ca.Length];
        var cModB = new bool[cb.Length];
        new Runner(ca, cb, cModA, cModB).Lcs(0, ca.Length, 0, cb.Length);

        for (int i = 0; i < ca.Length; i++) if (cModA[i]) modA[idxA[i]] = true;
        for (int j = 0; j < cb.Length; j++) if (cModB[j]) modB[idxB[j]] = true;
        return (modA, modB);
    }

    private sealed class Runner
    {
        private readonly int[] _a, _b;
        private readonly bool[] _modA, _modB;
        private readonly int[] _down, _up;
        private readonly int _max;

        public Runner(int[] a, int[] b, bool[] modA, bool[] modB)
        {
            _a = a; _b = b; _modA = modA; _modB = modB;
            _max = a.Length + b.Length + 1;
            _down = new int[2 * _max + 2];
            _up = new int[2 * _max + 2];
        }

        public void Lcs(int lowerA, int upperA, int lowerB, int upperB)
        {
            while (lowerA < upperA && lowerB < upperB && _a[lowerA] == _b[lowerB]) { lowerA++; lowerB++; }
            while (lowerA < upperA && lowerB < upperB && _a[upperA - 1] == _b[upperB - 1]) { upperA--; upperB--; }

            if (lowerA == upperA)
            {
                while (lowerB < upperB) _modB[lowerB++] = true;
            }
            else if (lowerB == upperB)
            {
                while (lowerA < upperA) _modA[lowerA++] = true;
            }
            else
            {
                var (x, y) = MiddleSnake(lowerA, upperA, lowerB, upperB);
                Lcs(lowerA, x, lowerB, y);
                Lcs(x, upperA, y, upperB);
            }
        }

        private (int X, int Y) MiddleSnake(int lowerA, int upperA, int lowerB, int upperB)
        {
            int downK = lowerA - lowerB;
            int upK = upperA - upperB;
            int delta = (upperA - lowerA) - (upperB - lowerB);
            bool oddDelta = (delta & 1) != 0;
            int downOffset = _max - downK;
            int upOffset = _max - upK;
            int maxD = (upperA - lowerA + upperB - lowerB) / 2 + 1;
            long size = upperA - lowerA + upperB - lowerB;

            _down[downOffset + downK + 1] = lowerA;
            _up[upOffset + upK - 1] = upperA;

            for (int d = 0; d <= maxD; d++)
            {
                if ((long)d * size > MaxWorkPerSnake)
                    return ((lowerA + upperA) / 2, (lowerB + upperB) / 2);

                for (int k = downK - d; k <= downK + d; k += 2)
                {
                    int x;
                    if (k == downK - d)
                    {
                        x = _down[downOffset + k + 1];
                    }
                    else
                    {
                        x = _down[downOffset + k - 1] + 1;
                        if (k < downK + d && _down[downOffset + k + 1] >= x) x = _down[downOffset + k + 1];
                    }
                    int y = x - k;
                    while (x < upperA && y < upperB && _a[x] == _b[y]) { x++; y++; }
                    _down[downOffset + k] = x;

                    if (oddDelta && upK - d < k && k < upK + d && _up[upOffset + k] <= _down[downOffset + k])
                        return (_down[downOffset + k], _down[downOffset + k] - k);
                }

                for (int k = upK - d; k <= upK + d; k += 2)
                {
                    int x;
                    if (k == upK + d)
                    {
                        x = _up[upOffset + k - 1];
                    }
                    else
                    {
                        x = _up[upOffset + k + 1] - 1;
                        if (k > upK - d && _up[upOffset + k - 1] < x) x = _up[upOffset + k - 1];
                    }
                    int y = x - k;
                    while (x > lowerA && y > lowerB && _a[x - 1] == _b[y - 1]) { x--; y--; }
                    _up[upOffset + k] = x;

                    if (!oddDelta && downK - d <= k && k <= downK + d && _up[upOffset + k] <= _down[downOffset + k])
                        return (_down[downOffset + k], _down[downOffset + k] - k);
                }
            }

            throw new InvalidOperationException("Middle snake not found.");
        }
    }
}
