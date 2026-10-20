/**
 * The budgets of the write path.
 *
 * The defaults come from `docs/11-write-path/01-write-path-design.md`
 * section 3: a plan is a change set, not a script. Every budget is a hard
 * limit, so a plan that exceeds one is refused before the first mutation.
 */

import { writeError } from "./errors.js";
import { WRITE_ERROR_CODES, WRITE_ERROR_REASONS, type WriteErrorDetail } from "./types.js";

/** The limits one plan has to stay inside. */
export interface WriteBudgets {
  /** Maximum number of operations in one plan. */
  maxOperations: number;
  /** Maximum number of nodes one plan creates. */
  maxCreatedNodes: number;
  /** Maximum number of characters one plan writes. */
  maxTextCharacters: number;
  /** Maximum number of property changes one plan makes. */
  maxPropertyChanges: number;
  /** Maximum number of nodes one delete takes with it. */
  maxDeleteSubtree: number;
}

/** The default budgets of the write path. */
export const DEFAULT_WRITE_BUDGETS: WriteBudgets = {
  maxOperations: 50,
  maxCreatedNodes: 100,
  maxTextCharacters: 20000,
  maxPropertyChanges: 200,
  maxDeleteSubtree: 50,
};

/**
 * The usage of the budgets of one plan.
 *
 * The ledger counts while the plan is resolved, so the breach is known before
 * the first mutation runs.
 */
export class BudgetLedger {
  private operations = 0;
  private created = 0;
  private textCharacters = 0;
  private propertyChanges = 0;

  constructor(private readonly budgets: WriteBudgets = DEFAULT_WRITE_BUDGETS) {}

  /** The limits the ledger enforces. */
  get limits(): WriteBudgets {
    return this.budgets;
  }

  /** Count one operation of the plan. */
  countOperation(): void {
    this.operations += 1;
  }

  /** Count one created node. */
  countCreatedNode(): void {
    this.created += 1;
  }

  /** Count the characters of a text change. */
  countTextCharacters(characters: number): void {
    this.textCharacters += characters;
  }

  /** Count the property changes of an operation. */
  countPropertyChanges(changes: number): void {
    this.propertyChanges += changes;
  }

  /**
   * The first budget the plan exceeds, when it exceeds one.
   *
   * The detail names the limit, so the caller can tell the agent which
   * budget has to shrink.
   */
  breach(operation: number | null = null): WriteErrorDetail | null {
    if (this.operations > this.budgets.maxOperations) {
      return budgetBreach(
        `The plan holds more than ${this.budgets.maxOperations} operations`,
        operation,
      );
    }
    if (this.created > this.budgets.maxCreatedNodes) {
      return budgetBreach(
        `The plan creates more than ${this.budgets.maxCreatedNodes} nodes`,
        operation,
      );
    }
    if (this.textCharacters > this.budgets.maxTextCharacters) {
      return budgetBreach(
        `The plan writes more than ${this.budgets.maxTextCharacters} text characters`,
        operation,
      );
    }
    if (this.propertyChanges > this.budgets.maxPropertyChanges) {
      return budgetBreach(
        `The plan changes more than ${this.budgets.maxPropertyChanges} properties`,
        operation,
      );
    }
    return null;
  }

  /**
   * Check the size of a subtree a delete would take with it.
   *
   * The limit is per delete: a delete that swallows a library is not a design
   * change.
   */
  checkDeleteSubtree(size: number, operation: number | null = null): WriteErrorDetail | null {
    if (size > this.budgets.maxDeleteSubtree) {
      return writeError(
        WRITE_ERROR_CODES.BUDGET_EXCEEDED,
        WRITE_ERROR_REASONS.BUDGET_EXCEEDED,
        `The delete takes more than ${this.budgets.maxDeleteSubtree} nodes with it`,
        { operation },
      );
    }
    return null;
  }
}

/** Build the detail of a budget breach. */
function budgetBreach(message: string, operation: number | null): WriteErrorDetail {
  return writeError(WRITE_ERROR_CODES.BUDGET_EXCEEDED, WRITE_ERROR_REASONS.BUDGET_EXCEEDED, message, {
    operation,
  });
}
