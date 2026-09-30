using System;
using System.Diagnostics;
using System.Text;

// The C# side of `crates/rts-core/examples/string_cost.rs`, row for row.
//
// # How the allocation is kept honest
//
// .NET 9+ stack-allocates objects it can prove do not escape, so a loop that
// allocated and dropped would measure nothing. Nothing is stored here either —
// storing costs a write barrier, and the first version of this file measured
// 8.65 ns for that store alone, which is the same order as the allocation it
// was supposed to be holding still. What proves the allocation happened is the
// GEN0 COUNT printed beside each row: a stack allocation moves no budget.
//
// The sink is a field read out of the fresh object, which is what the RTS probe
// does with its `wrapping_add`.
static class Bench
{
    // 15 longs: the payload of an RTS cell (header 8 + 15 slots of 8 = 128 B).
    sealed class Cell15
    {
        public long S0, S1;
        public long S2, S3, S4, S5, S6, S7, S8, S9, S10, S11, S12, S13, S14;
    }

    // What .NET would really allocate for the same job: a slot and a length.
    sealed class Pair { public long A, B; }

    static Cell15 keptCell;
    static string keptText;

    const long EACH = 3_000_000;
    const int RUNS = 5;

    static void Report(string what, Func<long, long> body)
    {
        body(200_000);
        double best = double.MaxValue;
        long sink = 0;
        int gc0 = GC.CollectionCount(0);
        for (int r = 0; r < RUNS; r++)
        {
            var sw = Stopwatch.StartNew();
            sink += body(EACH);
            sw.Stop();
            double ns = sw.Elapsed.TotalMilliseconds * 1e6 / EACH;
            if (ns < best) best = ns;
        }
        gc0 = GC.CollectionCount(0) - gc0;
        Console.WriteLine($"{what,-32}{best,7:F2} ns/op   gen0 {gc0,5}   (sink {sink})");
    }

    static void Main()
    {
        Console.WriteLine($"runtime {Environment.Version}, server GC {System.Runtime.GCSettings.IsServerGC}, "
                        + $"{RUNS} runs of {EACH:N0}, best");
        Console.WriteLine();

        var chars = new char[] { 'a', 'b', 'c', 'd', 'e' };
        var bytes = new byte[] { 97, 98, 99, 100, 101 };
        var already = new Cell15();

        Report("floor: empty loop", n => { long a = 0; for (long i = 0; i < n; i++) a += i; return a; });

        Report("floor: static store (barrier)", n => {
            long a = 0;
            for (long i = 0; i < n; i++) { keptCell = already; a += i; }
            return a;
        });

        Report("new Cell15() 128 B", n => {
            long a = 0;
            for (long i = 0; i < n; i++) { var c = new Cell15(); a += c.S0 + i; }
            return a;
        });

        Report("new Cell15() + 2 writes", n => {
            long a = 0;
            for (long i = 0; i < n; i++) { var c = new Cell15(); c.S0 = i; c.S1 = 5; a += c.S1; }
            return a;
        });

        Report("new Pair() 32 B + 2 writes", n => {
            long a = 0;
            for (long i = 0; i < n; i++) { var p = new Pair(); p.A = i; p.B = 5; a += p.B; }
            return a;
        });

        Report("new string('a', 5)", n => {
            long a = 0;
            for (long i = 0; i < n; i++) { var s = new string('a', 5); a += s.Length; }
            return a;
        });

        Report("new string(char[5])", n => {
            long a = 0;
            for (long i = 0; i < n; i++) { var s = new string(chars); a += s.Length; }
            return a;
        });

        Report("Latin1.GetString(byte[5])", n => {
            long a = 0;
            for (long i = 0; i < n; i++) { var s = Encoding.Latin1.GetString(bytes); a += s.Length; }
            return a;
        });

        Report("string.Intern(fresh 5)", n => {
            long a = 0;
            for (long i = 0; i < n; i++) { var s = string.Intern(new string(chars)); a += s.Length; }
            return a;
        });

        Report("new string('a', 64)", n => {
            long a = 0;
            for (long i = 0; i < n; i++) { var s = new string('a', 64); a += s.Length; }
            return a;
        });

        Report("new string + static store", n => {
            long a = 0;
            for (long i = 0; i < n; i++) { var s = new string(chars); keptText = s; a += s.Length; }
            return a;
        });

        Report("(1.23456).ToString(F2)", n => {
            long a = 0; double x = 1.23456;
            for (long i = 0; i < n; i++) { var s = x.ToString("F2"); a += s.Length; }
            return a;
        });

        Report("Convert.ToString(i, 16)", n => {
            long a = 0;
            for (long i = 0; i < n; i++) { var s = Convert.ToString((int)(i & 65535), 16); a += s.Length; }
            return a;
        });

        Report("i.ToString() base ten", n => {
            long a = 0;
            for (long i = 0; i < n; i++) { var s = ((int)(i & 65535)).ToString(); a += s.Length; }
            return a;
        });

        GC.KeepAlive(keptCell);
        GC.KeepAlive(keptText);
    }
}
