import { useEffect, useState } from "react";
import type { ChatBubble } from "../lib/session";
import type { SeatPosition } from "../lib/tileUtils";
import { seatPosition } from "../lib/tileUtils";

interface Props {
  bubbles: ChatBubble[];
  localSeat: number;
  onExpire: (id: string) => void;
}

const positionStyle: Record<SeatPosition, React.CSSProperties> = {
  bottom: { bottom: "22%", left: "50%", transform: "translateX(-50%)" },
  top: { top: "22%", left: "50%", transform: "translateX(-50%)" },
  left: { left: "14%", top: "50%", transform: "translateY(-50%)" },
  right: { right: "14%", top: "50%", transform: "translateY(-50%)" },
};

export function ChatBubbles({ bubbles, localSeat, onExpire }: Props) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 500);
    return () => window.clearInterval(id);
  }, []);

  useEffect(() => {
    for (const b of bubbles) {
      if (now - b.sentAt > 3200) onExpire(b.messageId);
    }
  }, [bubbles, now, onExpire]);

  return (
    <>
      {bubbles.map((b) => (
        <div
          key={b.messageId}
          className="chat-bubble"
          style={positionStyle[seatPosition(b.actorSeat, localSeat)]}
        >
          {b.emoji}
        </div>
      ))}
    </>
  );
}
