export function formatBeijingDateTime(rawTime: string) {
  const date = new Date(rawTime);
  if (Number.isNaN(date.getTime())) {
    return rawTime;
  }

  const parts = new Intl.DateTimeFormat('zh-CN', {
    timeZone: 'Asia/Shanghai',
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  })
    .formatToParts(date)
    .reduce<Record<string, string>>((lookup, part) => {
      lookup[part.type] = part.value;
      return lookup;
    }, {});

  return `${parts.year}-${parts.month}-${parts.day} ${parts.hour}:${parts.minute}:${parts.second}`;
}
