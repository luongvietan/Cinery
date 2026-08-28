export function humanizeWorkflowStatus(status: string) {
  const phrase = status.split("_").join(" ");
  return phrase[0].toUpperCase() + phrase.slice(1);
}
