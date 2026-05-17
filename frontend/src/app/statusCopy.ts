export function getRejectedMessage(reason: string) {
  const lookup: Record<string, string> = {
    table_not_found: '牌桌不存在或已关闭。',
    table_closed: '牌桌已关闭，请返回大厅重新进入。',
    table_full: '本牌局人数已满。',
  };

  return lookup[reason] ?? '请求未被服务器接受，请按最新房间状态重试。';
}

export function getSocialStatusCopy(detail: string) {
  const lookup: Record<string, string> = {
    auth_required: '登录状态已失效，请重新登录。',
    invite_code_invalid: '邀请码无效或已被使用。',
    invalid_credentials: '账号或密码错误。',
    username_taken: '该账号名已被占用。',
    target_player_busy: '该玩家正在牌局中，请稍后重试。',
    target_already_in_table: '该玩家已在本牌局中。',
    only_owner_can_invite: '只有房主可以邀请玩家。',
    table_multiplier_locked: '牌局已开始，无法再修改牌局设置。',
    table_not_found: '牌桌不存在或已关闭。',
  };

  return lookup[detail] ?? detail;
}

