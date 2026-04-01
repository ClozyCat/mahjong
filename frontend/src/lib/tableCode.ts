const TABLE_CODE_PATTERN = /^[A-Z0-9]+$/;
export const TABLE_CODE_MAX_LENGTH = 12;

export function normalizeTableCode(value: string) {
  return value.trim().toUpperCase();
}

export function getTableCodeError(value: string, options?: { required?: boolean }) {
  const normalizedValue = normalizeTableCode(value);

  if (!normalizedValue) {
    return options?.required ? '请输入牌桌编号。' : null;
  }

  if (normalizedValue.length > TABLE_CODE_MAX_LENGTH) {
    return `牌桌编号最多 ${TABLE_CODE_MAX_LENGTH} 位。`;
  }

  if (!TABLE_CODE_PATTERN.test(normalizedValue)) {
    return '牌桌编号仅支持数字和英文字母。';
  }

  return null;
}

export function isTableCodeValid(value: string, options?: { required?: boolean }) {
  return getTableCodeError(value, options) === null;
}
