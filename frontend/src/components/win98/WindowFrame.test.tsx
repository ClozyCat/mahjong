import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { WindowFrame } from './WindowFrame';

describe('WindowFrame', () => {
  it('renders a title bar, content region, and optional status bar', () => {
    render(
      <WindowFrame title="四风麻将客户端" status="等待连接">
        <div>内容区</div>
      </WindowFrame>,
    );

    expect(screen.getByText('四风麻将客户端')).toBeInTheDocument();
    expect(screen.getByText('内容区')).toBeInTheDocument();
    expect(screen.getByText('等待连接')).toBeInTheDocument();
  });
});
