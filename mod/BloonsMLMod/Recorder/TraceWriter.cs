using System;
using System.Collections.Concurrent;
using System.IO;
using System.IO.Compression;
using System.Text;
using System.Threading;

namespace BloonsML.Recorder;

/// <summary>
/// Schreibt Trace-Zeilen (JSONL, gzip) in einem Hintergrundthread. Der Spiel-Thread stellt nur Strings in eine
/// Queue, damit Datei-IO das Spiel (und damit die Messung) nicht beeinflusst.
/// </summary>
public sealed class TraceWriter : IDisposable
{
    private readonly BlockingCollection<string> queue = new(new ConcurrentQueue<string>());
    private readonly Thread thread;

    public string Path { get; }
    public long Lines { get; private set; }

    public TraceWriter(string path)
    {
        Path = path;
        Directory.CreateDirectory(System.IO.Path.GetDirectoryName(path)!);
        thread = new Thread(Run) { IsBackground = true, Name = "BloonsML-TraceWriter" };
        thread.Start();
    }

    public void Enqueue(string line)
    {
        if (!queue.IsAddingCompleted) queue.Add(line);
    }

    private void Run()
    {
        try
        {
            using var file = File.Create(Path);
            using var gz = new GZipStream(file, CompressionLevel.Fastest);
            using var writer = new StreamWriter(gz, new UTF8Encoding(false));
            foreach (var line in queue.GetConsumingEnumerable())
            {
                writer.Write(line);
                writer.Write('\n');
                Lines++;
            }
        }
        catch (Exception e)
        {
            BloonsMLMod.Log.Error($"Trace-Datei {Path}: {e}");
        }
    }

    /// <summary>Queue schließen und warten, bis alles geschrieben ist.</summary>
    public void Dispose()
    {
        queue.CompleteAdding();
        thread.Join(TimeSpan.FromSeconds(10));
        queue.Dispose();
    }
}
