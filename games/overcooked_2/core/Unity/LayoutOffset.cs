using UnityEngine;

namespace Swofty.Overcooked2.Core
{
    internal sealed class LayoutOffset : MonoBehaviour
    {
        private const float Tolerance = 0.0001f;

        private Vector2 offset;
        private float scale = 1f;
        private bool applied;
        private Vector2 rawPosition;
        private Vector3 rawScale;
        private Vector2 lastPosition;
        private Vector3 lastScale;

        internal static void Attach(GameObject go, Vector2 offset, float scale)
        {
            LayoutOffset o = go.GetComponent<LayoutOffset>();
            if (o == null)
            {
                o = go.AddComponent<LayoutOffset>();
            }
            o.offset = offset;
            o.scale = scale;
            o.Apply();
        }

        private void LateUpdate()
        {
            Apply();
        }

        private void Apply()
        {
            RectTransform rt = transform as RectTransform;
            if (rt == null)
            {
                return;
            }
            Vector2 p = rt.anchoredPosition;
            Vector3 s = rt.localScale;
            if (!applied || (p - lastPosition).sqrMagnitude > Tolerance)
            {
                rawPosition = p;
            }
            if (!applied || (s - lastScale).sqrMagnitude > Tolerance)
            {
                rawScale = s;
            }
            lastPosition = rawPosition + offset;
            lastScale = new Vector3(rawScale.x * scale, rawScale.y * scale, rawScale.z);
            rt.anchoredPosition = lastPosition;
            rt.localScale = lastScale;
            applied = true;
        }
    }
}
