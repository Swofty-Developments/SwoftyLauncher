using System;
using System.Collections.Generic;
using System.Reflection;
using HarmonyLib;
using UnityEngine;
using UnityEngine.SceneManagement;

namespace Swofty.Overcooked2.Core
{
    internal static class GameHooks
    {
        internal const string HarmonyId = "swofty.core";

        private static CoreHost host;
        private static Harmony harmony;
        private static bool firstScene = true;

        internal static CoreHost Host
        {
            get { return host; }
        }

        internal static void Install(CoreHost coreHost)
        {
            host = coreHost;
            harmony = new Harmony(HarmonyId);
            Game.Version = ReadGameVersion();
            host.Log.Info("Overcooked 2 build " + Game.Version);
            Dialogs.Backend = new DialogBackend();
            MainMenu.Backend = MenuInjector.Instance;
            MenuInjector.Install(harmony, host);
            SceneManager.sceneLoaded += OnSceneLoaded;
            MainMenu.AddButton("swofty.addons", "Addons", AddonsDialog.Show);
        }

        private static string ReadGameVersion()
        {
            try
            {
                Type t = AccessTools.TypeByName("BuildVersion");
                FieldInfo f = t == null ? null : AccessTools.Field(t, "m_VersionString");
                string v = f == null ? null : f.GetValue(null) as string;
                return string.IsNullOrEmpty(v) ? "unknown" : v;
            }
            catch (Exception)
            {
                return "unknown";
            }
        }

        private static void OnSceneLoaded(Scene scene, LoadSceneMode mode)
        {
            if (firstScene)
            {
                firstScene = false;
                host.Log.AttachUnity(UnityLog);
                CoreBehaviour.Create();
                host.Log.Info("first scene " + scene.name + "; loading addons");
                host.LoadAddons();
            }
            else
            {
                host.Log.Info("scene " + scene.name + " loaded (" + mode + ")");
            }
            SceneDump.OnSceneLoaded(scene.name);
            Game.RaiseSceneLoaded(scene.name);
        }

        private static void UnityLog(LogLevel level, string line)
        {
            switch (level)
            {
                case LogLevel.Error:
                    Debug.LogError(line);
                    break;
                case LogLevel.Warn:
                    Debug.LogWarning(line);
                    break;
                default:
                    Debug.Log(line);
                    break;
            }
        }
    }

    internal sealed class CoreBehaviour : MonoBehaviour
    {
        private static readonly List<Action> queue = new List<Action>();
        private static CoreBehaviour instance;

        internal static void Create()
        {
            if (instance != null)
            {
                return;
            }
            GameObject go = new GameObject("Swofty Core");
            UnityEngine.Object.DontDestroyOnLoad(go);
            go.hideFlags = HideFlags.HideAndDontSave;
            instance = go.AddComponent<CoreBehaviour>();
        }

        internal static void Post(Action action)
        {
            lock (queue)
            {
                queue.Add(action);
            }
        }

        private void Update()
        {
            Action[] pending;
            lock (queue)
            {
                if (queue.Count == 0)
                {
                    pending = null;
                }
                else
                {
                    pending = queue.ToArray();
                    queue.Clear();
                }
            }
            if (pending != null)
            {
                foreach (Action a in pending)
                {
                    Events.Invoke(a, "a queued core action");
                }
            }
            DialogBackend.Pump();
            SceneDump.Tick();
        }
    }
}
