import { useState } from "react";

const EMOJIS = ["😄", "🙂", "🙏", "👏", "👀", "😅", "🤔", "🎉"];

interface Props {
  localSeat: number;
  seats: { seat_index: number; nickname: string | null; is_bot: boolean }[];
  onSend: (target: number, emoji: string) => void;
}

export function QuickChat({ localSeat, seats, onSend }: Props) {
  const [open, setOpen] = useState(false);
  const [target, setTarget] = useState<number>(localSeat);

  return (
    <>
      <button
        type="button"
        className="quick-chat-toggle glass"
        onClick={() => setOpen((x) => !x)}
        title="快捷表情"
      >
        ☺
      </button>
      {open ? (
        <div className="quick-chat-panel">
          <div
            style={{
              gridColumn: "span 4",
              display: "flex",
              gap: 6,
              flexWrap: "wrap",
              marginBottom: 4,
            }}
          >
            {seats.map((s) => (
              <button
                key={s.seat_index}
                type="button"
                className="variant-chip"
                style={{
                  opacity: target === s.seat_index ? 1 : 0.5,
                  padding: "4px 10px",
                  fontSize: 11,
                }}
                onClick={() => setTarget(s.seat_index)}
              >
                {s.nickname ?? `座位${s.seat_index + 1}`}
              </button>
            ))}
          </div>
          {EMOJIS.map((e) => (
            <button
              key={e}
              type="button"
              className="emoji-btn"
              onClick={() => {
                onSend(target, e);
                setOpen(false);
              }}
            >
              {e}
            </button>
          ))}
        </div>
      ) : null}
    </>
  );
}
