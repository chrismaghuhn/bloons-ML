"""Faltungs-Policy: Aktions-Layout der Logits, bf16-Pfad und compile-Rückfall (CPU-tauglich)."""
import pytest

torch = pytest.importorskip("torch")
pytest.importorskip("sb3_contrib")

from btd_sim.policy import GridNet, grid_spec  # noqa: E402

GRID = dict(channels=3, height=4, width=5, scalars=2, heads=3)


def test_cell_logits_are_heads_major_then_row_major():
    net = GridNet(**GRID, width_ch=8, blocks=1)
    obs = torch.rand(2, 3 * 20 + 2)
    x, pooled = net._features(obs)
    cells = net.cell_head(x)  # (n, heads, H, W)
    logits = net._logits(x, pooled)
    assert logits.shape == (2, 1 + 3 * 20)
    for h, r, c in [(0, 0, 0), (1, 2, 3), (2, 3, 4)]:
        assert torch.equal(logits[:, 1 + h * 20 + r * 5 + c], cells[:, h, r, c])


def test_amp_layout_and_dtype_like_f32():
    torch.manual_seed(0)
    f32 = GridNet(**GRID, width_ch=8, blocks=1)
    amp = GridNet(**GRID, width_ch=8, blocks=1, amp=True)
    amp.load_state_dict(f32.state_dict())
    obs = torch.rand(2, 3 * 20 + 2)
    (l1, v1), (l2, v2) = f32(obs), amp(obs)  # auf CPU ohne Autocast, nur channels_last
    assert l2.dtype == v2.dtype == torch.float32
    assert torch.allclose(l1, l2, atol=1e-5) and torch.allclose(v1, v2, atol=1e-5)


def test_try_compile_falls_back(monkeypatch):
    net = GridNet(**GRID, width_ch=8, blocks=1)

    def broken(f):
        def g(*a):
            raise RuntimeError("kein Compiler")
        return g

    monkeypatch.setattr(torch, "compile", broken)
    with pytest.warns(UserWarning):
        assert net.try_compile("cpu") is False
    net(torch.rand(1, 3 * 20 + 2))  # läuft weiter unkompiliert


def test_grid_spec_adds_buff_channels_for_support_towers(tmp_path):
    """Mit Support-Türmen (Village/Farm) hat die Rust-Env 5 Buff-Kanäle mehr (Layout::support)."""
    import json
    tower = {"name": "dart", "upgrade_costs": [[1], [], []], "variants": [{"tiers": [0, 0, 0]}]}
    village = {"name": "village", "upgrade_costs": [[], [], []],
               "variants": [{"tiers": [0, 0, 0], "supports": [{"kind": "camo", "group": "V"}]}]}
    farm = {"name": "farm", "upgrade_costs": [[], [], []],
            "variants": [{"tiers": [0, 0, 0], "income": {"per_round": 80.0}}]}
    shop = {"name": "shop", "upgrade_costs": [[], [], []],
            "variants": [{"tiers": [0, 0, 0], "discount": [{"fraction": 0.1}]}]}
    for towers, channels in [([tower], 2 + 1 + 1), ([tower, village], 2 + 2 + 1 + 5),
                             ([tower, farm], 2 + 2 + 1 + 5), ([tower, shop], 2 + 2 + 1 + 5)]:
        path = tmp_path / "d.json"
        path.write_text(json.dumps({"map": {"grid": {"h": 3, "w": 4}}, "towers": towers}))
        heads = len(towers) + 1 + 1
        spec = grid_spec(str(path), channels * 12 + 7, 1 + heads * 12)
        assert (spec["channels"], spec["scalars"]) == (channels, 7)
    with pytest.raises(ValueError, match="Layout"):  # 5 Kanäle zu viel als Skalare gezählt
        grid_spec(str(path), (channels + 5) * 12 + 7, 1 + heads * 12)
