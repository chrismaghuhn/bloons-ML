using System.IO;
using System.Text;
using BTD_Mod_Helper.Api.Internal;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using Il2CppObject = Il2CppSystem.Object;

namespace BloonsML.Common;

/// <summary>
/// Serialisierung von Spiel-Models (Il2Cpp-Objekten) nach JSON.
/// Nutzt den Serializer von BTD6 Mod Helper, der Il2Cpp-Typen, Unity-Vektoren und
/// die Model-Hierarchie (Behaviors, Weapons, Projectiles, ...) generisch abbildet und an
/// jedem Objekt "$type" mit dem Spiel-Typnamen hinterlegt.
/// </summary>
public static class ModelJson
{
    private static readonly UTF8Encoding Utf8NoBom = new(false);

    public static JToken ToJToken(Il2CppObject obj) =>
        obj == null ? JValue.CreateNull() : JToken.FromObject(obj, Il2CppJsonConvert.Serializer);

    /// <summary>Für beliebige Werte (auch Strings/Zahlen/Enums aus Il2Cpp-Properties).</summary>
    public static JToken ToJTokenAny(object value) =>
        value == null ? JValue.CreateNull() : JToken.FromObject(value, Il2CppJsonConvert.Serializer);

    public static void Write(string path, JToken token, bool indented = true)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        File.WriteAllText(path, token.ToString(indented ? Formatting.Indented : Formatting.None), Utf8NoBom);
    }
}
