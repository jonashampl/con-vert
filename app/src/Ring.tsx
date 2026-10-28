import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Archive, Crop, EyeOff, Gauge, Image, ImageDown, PenLine, Scissors, Shrink, SlidersHorizontal, Tag, Volume2, Waves, type LucideIcon } from "lucide-react";
import "./ring.css";

// Geometry in logical px. Must match trigger.rs (WIN_LOGICAL, RING_R_LOGICAL)
// and core/src/geometry.rs (HUB_RATIO), which do the hit-testing.
const WIN = 384;
const C = WIN / 2;
const R = 180;
const HUB = 0.34 * R;
const PETAL_IN = 0.4 * R;
const PETAL_OUT = 0.93 * R;
const CORNER = 12;
const GAP = 7;
const PLACEHOLDER_SEGMENTS = 8;

type Segment = { id: string; label: string; icon: string | null; enabled: boolean };
type Menu = { pages: [Segment[], Segment[]]; start_page: number; title: string; media: boolean };
type RingEvent =
  | { type: "show"; backdrop: string | null; beaver: [number, number, number] | null }
  | { type: "held" }
  | { type: "menu"; menu: Menu; page: number }
  | { type: "page"; page: number }
  | { type: "hover"; index: number }
  | { type: "thumb"; src: string }
  | { type: "chosen"; index: number }
  | { type: "zip"; on: boolean }
  | { type: "hide" };

const ICONS: Record<string, LucideIcon> = {
  compress: Shrink,
  tag: Tag,
  sliders: SlidersHorizontal,
  pen: PenLine,
  image: Image,
  crop: Crop,
  "eye-off": EyeOff,
  archive: Archive,
  gauge: Gauge,
  volume: Volume2,
  scissors: Scissors,
  waves: Waves,
  "image-down": ImageDown,
};

/** Audio and video files: a record spinning in the hub, in place of a thumbnail. */
function Vinyl() {
  const r = HUB * 0.5;
  const cy = C - HUB * 0.3;
  return (
    <g className="vinyl">
      <circle cx={C} cy={cy} r={r} fill="#0d0d10" />
      {[0.84, 0.68].map((k) => (
        <circle key={k} cx={C} cy={cy} r={r * k} fill="none" stroke="rgba(255,255,255,0.09)" strokeWidth={1} />
      ))}
      <path
        d={`M ${C} ${cy - r * 0.92} A ${r * 0.92} ${r * 0.92} 0 0 1 ${C + r * 0.86} ${cy - r * 0.33}`}
        fill="none"
        stroke="rgba(255,255,255,0.24)"
        strokeWidth={2}
        strokeLinecap="round"
      />
      <circle cx={C} cy={cy} r={r * 0.37} fill="#0a84ff" />
      <circle cx={C} cy={cy} r={r * 0.08} fill="#0d0d10" />
    </g>
  );
}

const point = (r: number, a: number) => `${(C + r * Math.sin(a)).toFixed(2)} ${(C - r * Math.cos(a)).toFixed(2)}`;

/**
 * Petal i of n, clockwise from the top. The path is inset by CORNER and drawn with a
 * round-joined stroke of 2*CORNER, which grows it back out with rounded corners.
 */
function petalPath(i: number, n: number) {
  const mid = (i * 2 * Math.PI) / n;
  const half = Math.PI / n;
  const inset = GAP / 2 + CORNER;
  const ri = PETAL_IN + CORNER;
  const ro = PETAL_OUT - CORNER;
  const ai = Math.asin(Math.min(1, inset / ri));
  const ao = Math.asin(Math.min(1, inset / ro));
  return [
    `M ${point(ri, mid - half + ai)}`,
    `L ${point(ro, mid - half + ao)}`,
    `A ${ro} ${ro} 0 0 1 ${point(ro, mid + half - ao)}`,
    `L ${point(ri, mid + half - ai)}`,
    `A ${ri} ${ri} 0 0 0 ${point(ri, mid - half + ai)}`,
    "Z",
  ].join(" ");
}

function PetalLabel({ seg, i, n }: { seg: Segment; i: number; n: number }) {
  const a = (i * 2 * Math.PI) / n;
  const r = (PETAL_IN + PETAL_OUT) / 2;
  const x = C + r * Math.sin(a);
  const y = C - r * Math.cos(a);
  const Icon = seg.icon ? ICONS[seg.icon] : undefined;
  if (!Icon) {
    return (
      <text className="label format" x={x} y={y} textAnchor="middle" dominantBaseline="central">
        {seg.label}
      </text>
    );
  }
  return (
    <>
      <Icon className="icon" x={x - 9} y={y - 19} width={18} height={18} strokeWidth={1.9} />
      <text className="label tool" x={x} y={y + 11} textAnchor="middle" dominantBaseline="central">
        {seg.label}
      </text>
    </>
  );
}

export default function Ring() {
  // Increments on every show so the entry animation replays; 0 = hidden.
  const [shown, setShown] = useState(0);
  const [backdrop, setBackdrop] = useState<string | null>(null);
  const [menu, setMenu] = useState<Menu | null>(null);
  const [page, setPage] = useState(0);
  const [hover, setHover] = useState(-1);
  const [thumb, setThumb] = useState<string | null>(null);
  const [chosen, setChosen] = useState(-1);
  // Several files on the Convert page: save them separately or as one ZIP.
  const [zip, setZip] = useState(false);
  // Where the floating beaver sits under the ring while a file is dragged from it, so you
  // can see where to let go; null once the file is down (then you click a petal).
  const [beaver, setBeaver] = useState<[number, number, number] | null>(null);

  useEffect(() => {
    document.documentElement.classList.add("ring-root");
    const un = listen<RingEvent>("ring", ({ payload: e }) => {
      switch (e.type) {
        case "show":
          setShown((s) => s + 1);
          setBackdrop(e.backdrop);
          setMenu(null);
          setPage(0);
          setHover(-1);
          setThumb(null);
          setChosen(-1);
          setBeaver(e.beaver);
          break;
        case "held":
          setBeaver(null);
          break;
        case "menu":
          setMenu(e.menu);
          setPage(e.page);
          break;
        case "page":
          setPage(e.page);
          setHover(-1);
          break;
        case "hover":
          setHover(e.index);
          break;
        case "thumb":
          setThumb(e.src);
          break;
        case "chosen":
          setChosen(e.index);
          break;
        case "zip":
          setZip(e.on);
          break;
        case "hide":
          setShown(0);
          break;
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  if (!shown) return null;

  const segs: Segment[] = menu
    ? menu.pages[page]
    : Array.from({ length: PLACEHOLDER_SEGMENTS }, (_, i) => ({ id: `p${i}`, label: "", icon: null, enabled: false }));
  const n = segs.length;
  const hovered = hover >= 0 ? segs[hover] : undefined;
  const pill = hovered?.enabled ? hovered.label : menu?.title ?? "";
  // Only worth drawing when the ring was pushed off-centre and covers the beaver.
  const marker = beaver && Math.hypot(beaver[0] - C, beaver[1] - C) > HUB ? beaver : null;

  return (
    <svg className="ring" key={shown} viewBox={`0 0 ${WIN} ${WIN}`}>
      <defs>
        <clipPath id="disc">
          <circle cx={C} cy={C} r={R} />
        </clipPath>
        <clipPath id="hub">
          <circle cx={C} cy={C} r={HUB - 1} />
        </clipPath>
        <linearGradient id="hot" gradientUnits="userSpaceOnUse" x1="0" y1={C - R} x2="0" y2={C + R}>
          <stop offset="0" stopColor="#1a6fef" />
          <stop offset="1" stopColor="#0058d6" />
        </linearGradient>
      </defs>

      <g clipPath="url(#disc)">
        {backdrop && (
          <image
            className="backdrop"
            href={backdrop}
            x={C - R - 16}
            y={C - R - 16}
            width={2 * R + 32}
            height={2 * R + 32}
            preserveAspectRatio="none"
          />
        )}
        <circle className="tint" cx={C} cy={C} r={R} />
      </g>
      <circle className="rim" cx={C} cy={C} r={R - 0.75} />

      <g className={`petals ${menu ? "" : "loading"}`} key={`${page}-${menu ? "m" : "p"}`}>
        {segs.map((s, i) => (
          <g
            key={s.id}
            className={["petal", i === hover && s.enabled && "hot", !s.enabled && menu && "off", i === chosen && "chosen"]
              .filter(Boolean)
              .join(" ")}
          >
            <path d={petalPath(i, n)} />
            {s.label && <PetalLabel seg={s} i={i} n={n} />}
          </g>
        ))}
      </g>

      <circle className="hub" cx={C} cy={C} r={HUB} />
      {menu?.media && <Vinyl />}
      {thumb && !menu?.media && (
        <image
          className="thumb"
          href={thumb}
          x={C - HUB * 0.62}
          y={C - HUB * 0.78}
          width={HUB * 1.24}
          height={HUB * 1.24}
          clipPath="url(#hub)"
          preserveAspectRatio="xMidYMid meet"
        />
      )}
      {pill && (
        <foreignObject x={C - 80} y={C - 15} width={160} height={30}>
          <div className="pill-wrap">
            <div className={`pill ${hovered?.enabled ? "action" : ""}`}>{pill}</div>
          </div>
        </foreignObject>
      )}
      {menu && page === 0 && menu.title.endsWith("files") && (
        // Same box as core geometry::on_zip_switch: |dx| <= 0.22 R, dy 0.10..0.24 R.
        <foreignObject x={C - 0.22 * R} y={C + 0.1 * R} width={0.44 * R} height={0.14 * R}>
          <div className="zip-switch" title="Save the results as one ZIP">
            <span className={zip ? "" : "on"}>Files</span>
            <span className={zip ? "on" : ""}>ZIP</span>
          </div>
        </foreignObject>
      )}
      {marker && (
        <g className="beaver-spot">
          <circle cx={marker[0]} cy={marker[1]} r={marker[2] * 0.82} />
          <image
            href="/icon.svg"
            x={marker[0] - marker[2] * 0.62}
            y={marker[1] - marker[2] * 0.62}
            width={marker[2] * 1.24}
            height={marker[2] * 1.24}
          />
        </g>
      )}
    </svg>
  );
}
