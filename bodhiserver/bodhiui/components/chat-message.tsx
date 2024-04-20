import { ChatMessageActions } from "@/components/chat-message-actions";
import { cn } from "@/lib/utils";
import { CodeBlock } from "@/components/ui/codeblock";
import { IconUser, IconOpenAI } from '@/components/ui/icons'
import { MemoizedReactMarkdown } from "@/components/ui/markdown";
import { type Message } from "ai/react";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";

export interface ChatMessageProps {
  message: Message
}

export function ChatMessage({ message }: ChatMessageProps) {
  return (<div
    className={cn('relative mb-4 flex items-start md:-ml-12')}
  >
    <div
      className={cn(
        'flex size-8 shrink-0 select-none items-center justify-center rounded-md border shadow',
        message.role === 'user'
          ? 'bg-background'
          : 'bg-primary text-primary-foreground'
      )}
    >
      {message.role === 'user' ? <IconUser /> : <IconOpenAI />}
    </div>
    <div className="group flex-1 px-1 ml-4 space-y-2 overflow-hidden">
      <MemoizedReactMarkdown
        className="prose break-words dark:prose-invert prose-p:leading-relaxed prose-pre:p-0"
        remarkPlugins={[remarkGfm, remarkMath]}
        components={{
          p({ children }) {
            return <p className="mb-2 last:mb-0">{children}</p>
          },
          code({ node, inline, className, children, ...props }) {
            if (children.length) {
              if (children[0] == '▍') {
                return (
                  <span className="mt-1 cursor-default animate-pulse">▍</span>
                )
              }

              children[0] = (children[0] as string).replace('`▍`', '▍')
            }

            const match = /language-(\w+)/.exec(className || '')

            if (inline) {
              return (
                <code className={className} {...props}>
                  {children}
                </code>
              )
            }

            return (
              <CodeBlock
                key={Math.random()}
                language={(match && match[1]) || ''}
                value={String(children).replace(/\n$/, '')}
                {...props}
              />
            )
          }
        }}
      >
        {message.content}
      </MemoizedReactMarkdown>
      <ChatMessageActions message={message} />
    </div>
  </div>)
}