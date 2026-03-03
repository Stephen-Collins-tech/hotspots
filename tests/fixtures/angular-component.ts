import { Component, OnInit, Input, Injectable } from '@angular/core';

@Injectable({ providedIn: 'root' })
export class UserService {
  getUser(id: number): string {
    if (id <= 0) {
      throw new Error('Invalid id');
    }
    return `user-${id}`;
  }
}

@Component({
  selector: 'app-user-card',
  template: `<div>{{ displayName }}</div>`,
})
export class UserCardComponent implements OnInit {
  @Input() userId: number = 0;
  displayName = '';

  constructor(private userService: UserService) {}

  ngOnInit(): void {
    this.displayName = this.userService.getUser(this.userId);
  }

  formatDisplay(prefix: string): string {
    if (!prefix) {
      return this.displayName;
    }
    return `${prefix}: ${this.displayName}`;
  }
}
