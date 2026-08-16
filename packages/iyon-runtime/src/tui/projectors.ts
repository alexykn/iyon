import { TextContent } from "./values/text-content.ts";
import { Projection } from "./values/projection.ts";

export class PlainTextProjector {
  project(content: TextContent): Projection { return new Projection(content, [{ sourceStart: 0, sourceEnd: content.text().length, text: content.text() }]); }
}

export class MarkdownProjector {
  project(content: TextContent): Projection {
    const text = content.text().replace(/\*\*([^*]+)\*\*/g, "$1").replace(/`([^`]+)`/g, "$1");
    return new Projection(content, [{ sourceStart: 0, sourceEnd: content.text().length, text }]);
  }
}
